use std::collections::{HashMap, HashSet};
use log::*;
use matches::matches;
use const_cstr::const_cstr;
use crate::document::model::*;
use crate::document::model;
use crate::document::analysis::*;
use crate::file;
use crate::app::*;
use crate::gui::widgets;
use std::sync::mpsc;

pub enum ImportError {
}


pub struct ImportWindow {
    pub open :bool,
    state :ImportState,
    thread :Option<mpsc::Receiver<ImportState>>,
    thread_pool :BackgroundJobs,
}

impl ImportWindow {
    pub fn new(thread_pool :BackgroundJobs) -> Self {
        ImportWindow {
            open: false,
            state: ImportState::ChooseFile,
            thread: None,
            thread_pool:thread_pool,
        }
    }
}

#[derive(Debug)]
pub enum ImportState {
    Ping,
    ChooseFile,
    ReadingFile,
    SourceFileError(String),
    PlotError(String),
    WaitForDrawing,
    Available(Model),
}

impl ImportWindow {
    pub fn open(&mut self) {
        self.open = true;
        self.state = ImportState::ChooseFile;
        self.thread = None;
    }

    pub fn update(&mut self) {
        while let Some(Ok(msg)) = self.thread.as_mut().map(|rx| rx.try_recv()) {
            println!("import window new  state: {:?}", msg);
            self.state = msg;
        }
    }

    pub fn draw(&mut self, doc :&mut Analysis) {
        if !self.open { return; }
        use backend_glfw::imgui::*;
        unsafe {
        widgets::next_window_center_when_appearing();
        igBegin(const_cstr!("Import from railML file").as_ptr(), &mut self.open as _, 0 as _);

        match &self.state {
            ImportState::ChooseFile => {
                if igButton(const_cstr!("Browse for file...").as_ptr(),
                            ImVec2 { x: 120.0, y: 0.0 }) {

                    if let Some(filename) = tinyfiledialogs::open_file_dialog("Select railML file.","", None) {
                        self.background_load_file(filename);
                    }
                }
            },

            ImportState::Available(model) => {
                if igButton(const_cstr!("Import").as_ptr(), ImVec2 { x: 80.0, y: 0.0 }) {
                    *doc = Analysis::from_model( model.clone(), self.thread_pool.clone());  
                    //doc.fileinfo.set_unsaved();
                    self.close();
                }
            },
            ImportState::Ping => { widgets::show_text("Running solver"); },
            x => { widgets::show_text(&format!("{:?}", x)); },
        }

        igEnd();
        }
    }

    pub fn background_load_file(&mut self, filename :String) {
        info!("Starting background loading of railml from file {:?}", filename);
        let (tx,rx) = mpsc::channel();
        self.thread = Some(rx);
        self.thread_pool.execute(|| { load_railml_file(filename, tx); });
    }

    pub fn close(&mut self) {
        self.open = false;
        self.state = ImportState::ChooseFile;
        self.thread = None;
    }
}

pub fn load_railml_file(filename :String, tx :mpsc::Sender<ImportState>)  {
    // outline of steps
    // 1. read file 
    // 2. convert to railml
    // 3. convert to topo
    // 4. convert to railplot model (directed topo with mileage)
    // 5. solve railplotlib
    // 6. convert to junction model (linesegments, nodes, objects/wlocations)

    let s = match std::fs::read_to_string(&filename) {
        Ok(s) => s,
        Err(e) => {
            let _ = tx.send(ImportState::SourceFileError(format!("Read error: {}", e)));
            return;
        }
    };
    if tx.send(ImportState::Ping).is_err() { return; }
    info!("Read file {:?}", filename);

    let parsed = match railmlio::xml::parse_railml(&s) {
        Ok(p) => p,
        Err(e) => {
            let _ = tx.send(ImportState::SourceFileError(format!("Parse error: {:?}", e)));
            return;
        },
    };
    if tx.send(ImportState::Ping).is_err() { return; }
    info!("Parsed railml");

    let topomodel = match railmlio::topo::convert_railml_topo(parsed) {
        Ok(m) => m,
        Err(e) => {
            println!("TOPMODEL ERR {:?}", e);
            let _ = tx.send(ImportState::SourceFileError(format!("Model conversion error: {:?}", e)));
            return;
        },
    };
    if tx.send(ImportState::Ping).is_err() { return; }
    info!("Converted to topomodel");

    let plotmodel = match convert_railplot(topomodel) {
        Ok(m) => m,
        Err(e) => {
            let _ = tx.send(e);
            return;
        },
    };
    if tx.send(ImportState::Ping).is_err() { return; }
    info!("Converted to plotmodel");

    let solver = railplotlib::solvers::LevelsSatSolver {
        criteria: vec![
            railplotlib::solvers::Goal::Bends,
            railplotlib::solvers::Goal::Height,
            railplotlib::solvers::Goal::Width,
            railplotlib::solvers::Goal::LocalX,
            railplotlib::solvers::Goal::LocalY,
        ],
        nodes_distinct: false,
    };
    use railplotlib::solvers::SchematicSolver;


    info!("Starting solver");
    info!("plot model {:#?}", plotmodel);
    let plot = match solver.solve(plotmodel) {
        Ok(m) => m,
        Err(e) => {
            let _ = tx.send(ImportState::PlotError(format!("Plotting error: {:?}", e)));
            return;
        },
    };
    if tx.send(ImportState::Ping).is_err() { return; }

    info!("Found model");
    let model = match convert_junction(plot) {
        Ok(m) => m,
        Err(e) => {
            let _ = tx.send(e);
            return;
        },
    };

    info!("Model available");
    let _ = tx.send(ImportState::Available(model));
}


#[derive(Debug, Clone, Copy)]
pub enum RailObject {
    Signal(railmlio::model::SignalType),
}

pub fn convert_railplot(topo :railmlio::topo::Topological) 
    -> Result<railplotlib::model::SchematicGraph<RailObject>, ImportState> {

    use railmlio::topo;
    use railplotlib::model as plot;

    enum MileageMethod { 
        /// Use the absolute position / mileage information
        /// in the railML file. This requires consistency between 
        /// absPos values on all elements, and the track directions,
        /// i.e. absPos values must be increasing along the track's direction.
        FromFile,

        /// Derive the mileage information by averaging track lengths on 
        /// all paths between locations.
        Estimated,
    }

    // prefer absolute positions when present; fall back otherwise
    let has_abs = topo.tracks.iter().any(|t| t.offset != 0.0 || t.length > 0.0);
    let method = if has_abs { MileageMethod::FromFile } else { MileageMethod::Estimated };

    match method {
        MileageMethod::FromFile => {
            // Use absPos on track ends/switches to set km0 directly.
            let mut model = plot::SchematicGraph {
                nodes: Vec::new(),
                edges: Vec::new(),
                main_tracks_edges: Vec::new(),
            };

            fn to_dir(dir :topo::AB) -> plot::Dir { 
                match dir {
                    topo::AB::A => plot::Dir::Up,
                    topo::AB::B => plot::Dir::Down,
                }
            }

            // nodes: track ends, switches, crossings, continuations
            let mut node_map: HashMap<usize, usize> = HashMap::new();
            for (node_idx,node_type) in topo.nodes.iter().enumerate() {
                let km0 = 0.0; // will adjust from track offsets
                if let topo::TopoNode::Continuation = node_type { continue; }
                let model_idx = model.nodes.len();
                model.nodes.push(plot::Node {
                    name: format!("n{}", node_idx),
                    pos: km0,
                    shape: match node_type {
                        topo::TopoNode::BufferStop | 
                        topo::TopoNode::OpenEnd | 
                        topo::TopoNode::MacroscopicNode => plot::Shape::Begin, // may flip to End later
                        topo::TopoNode::Switch(topo::Side::Left) => 
                            plot::Shape::Switch(plot::Side::Left, plot::Dir::Up), // dir adjusted later
                        topo::TopoNode::Switch(topo::Side::Right) => 
                            plot::Shape::Switch(plot::Side::Right, plot::Dir::Up), // dir adjusted later
                        topo::TopoNode::Crossing => plot::Shape::Crossing,
                        topo::TopoNode::Continuation => plot::Shape::Continuation,
                    }
                });
                node_map.insert(node_idx, model_idx);
            }

            let track_connections :HashMap<(usize,topo::AB),(usize,topo::Port)> = 
                topo.connections.iter().cloned().collect();
            let node_connections :HashMap<(usize,topo::Port),(usize,topo::AB)> = 
                topo.connections.iter().map(|(a,b)| (*b,*a)).collect();

            let mut edges_done = HashSet::new();
            let mut node_pos: HashMap<usize, f64> = HashMap::new();

            for (track_idx,track) in topo.tracks.iter().enumerate() {
                let mut na = track_connections.get(&(track_idx,topo::AB::A))
                    .ok_or(ImportState::SourceFileError(format!("Inconsistent connections.")))?;
                let mut nb = track_connections.get(&(track_idx,topo::AB::B))
                    .ok_or(ImportState::SourceFileError(format!("Inconsistent connections.")))?;

                fn cont_opposite(p :topo::Port) -> topo::Port {
                    match p {
                        topo::Port::ContA => topo::Port::ContB,
                        topo::Port::ContB => topo::Port::ContA,
                        x => x,
                    }
                }

                while let topo::Port::ContA | topo::Port::ContB = na.1 {
                    let (ti,tab) = node_connections.get(&(na.0, cont_opposite(na.1)))
                        .ok_or(ImportState::SourceFileError(format!("Inconsistent connections.")))?;
                    na = track_connections.get(&(*ti,tab.opposite()))
                        .ok_or(ImportState::SourceFileError(format!("Inconsistent connections.")))?;
                }
                while let topo::Port::ContA | topo::Port::ContB = nb.1 {
                    let (ti,tab) = node_connections.get(&(nb.0, cont_opposite(nb.1)))
                        .ok_or(ImportState::SourceFileError(format!("Inconsistent connections.")))?;
                    nb = track_connections.get(&(*ti,tab.opposite()))
                        .ok_or(ImportState::SourceFileError(format!("Inconsistent connections.")))?;
                }

                let convert_port = |(n,p) :(usize,topo::Port), is_lower: bool| {
                    match p {
                        topo::Port::Trunk => plot::Port::Trunk,
                        topo::Port::Left => plot::Port::Left,
                        topo::Port::Right => plot::Port::Right,
                        topo::Port::Single => if is_lower { plot::Port::Out } else { plot::Port::In },
                        topo::Port::Crossing(topo::AB::A, 0) => plot::Port::OutLeft,
                        topo::Port::Crossing(topo::AB::B, 0) => plot::Port::InLeft,
                        topo::Port::Crossing(topo::AB::A, 1) => plot::Port::OutRight,
                        topo::Port::Crossing(topo::AB::B, 1) => plot::Port::InRight,
                        _ => plot::Port::Out,
                }};

                let pa = convert_port(*na, true);
                let pb = convert_port(*nb, false);
                let mut a = (format!("n{}", na.0), pa);
                let mut b = (format!("n{}", nb.0), pb);

                // use abs positions: track.offset is from topo conversion
                let mut pos_a = track.offset;
                let mut pos_b = track.offset + track.length;
                if pos_a > pos_b {
                    std::mem::swap(&mut pos_a, &mut pos_b);
                    std::mem::swap(&mut a, &mut b);
                }

                let key = (a.clone(), b.clone());
                if !edges_done.contains(&key) {
                    edges_done.insert(key.clone());
                    let mut objects = Vec::new();
                    for s in &topo.tracks[track_idx].objects.signals {
                        objects.push((plot::Symbol {
                            pos: s.pos.offset,
                            width: 0.1,
                            origin: 0.0,
                            level: 1,
                        }, RailObject::Signal(s.r#type)));
                    }
                    if let Some(&mi) = node_map.get(&na.0) {
                        model.nodes[mi].pos = pos_a;
                        node_pos.insert(na.0, pos_a);
                    }
                    if let Some(&mi) = node_map.get(&nb.0) {
                        model.nodes[mi].pos = pos_b;
                        node_pos.insert(nb.0, pos_b);
                    }
                    model.edges.push(plot::Edge { a, b, objects });
                }
            }

            // flip boundary node shapes based on mileage ordering
            let mut higher_count: HashMap<usize, usize> = HashMap::new();
            let mut lower_count: HashMap<usize, usize> = HashMap::new();
            for edge in &model.edges {
                let idx_a: usize = edge.a.0.trim_start_matches('n').parse().unwrap_or(0);
                let idx_b: usize = edge.b.0.trim_start_matches('n').parse().unwrap_or(0);
                if let (Some(&ma), Some(&mb)) = (node_map.get(&idx_a), node_map.get(&idx_b)) {
                    let pa = model.nodes[ma].pos;
                    let pb = model.nodes[mb].pos;
                    if pa < pb {
                        *higher_count.entry(ma).or_insert(0) += 1;
                        *lower_count.entry(mb).or_insert(0) += 1;
                    } else if pb < pa {
                        *higher_count.entry(mb).or_insert(0) += 1;
                        *lower_count.entry(ma).or_insert(0) += 1;
                    }
                }
            }
            for (idx, node) in model.nodes.iter_mut().enumerate() {
                let hi = higher_count.get(&idx).cloned().unwrap_or(0);
                let lo = lower_count.get(&idx).cloned().unwrap_or(0);
                match node.shape {
                    plot::Shape::Begin | plot::Shape::End => {
                        if hi == 0 && lo > 0 {
                            node.shape = plot::Shape::End;
                        } else if hi > 0 && lo == 0 {
                            node.shape = plot::Shape::Begin;
                        }
                    },
                    plot::Shape::Switch(side, _) => {
                        // set direction based on majority: Up if most edges go to higher mileage
                        let dir = if hi >= lo { plot::Dir::Up } else { plot::Dir::Down };
                        node.shape = plot::Shape::Switch(side, dir);
                    },
                    _ => {},
                }
            }

            Ok(model)
        },
        MileageMethod::Estimated => {
            // start from any single node
            let start_node = topo.nodes.iter().position(|n| 
                                matches!(n, topo::TopoNode::BufferStop |
                                            topo::TopoNode::OpenEnd |
                                            topo::TopoNode::MacroscopicNode)).
                ok_or(ImportState::SourceFileError(format!("No entry/exit nodes found.")))?;

            type NodeId = usize; // index into topo.nodes

            let track_connections :HashMap<(usize,topo::AB),(usize,topo::Port)> = 
                topo.connections.iter().cloned().collect();
            debug!("Track connections {:?}", track_connections);
            let node_connections :HashMap<(usize,topo::Port),(usize,topo::AB)> = 
                topo.connections.iter().map(|(a,b)| (*b,*a)).collect();
            debug!("Node connections {:?}", node_connections);

            let mut km0 : HashMap<NodeId, (isize, f64)> = HashMap::new();
            let mut component_offset = 0.0;

            let mut node_indices : Vec<usize> = (0..topo.nodes.len()).collect();
            node_indices.sort_by_key(|&idx| !matches!(topo.nodes[idx], 
                topo::TopoNode::BufferStop | topo::TopoNode::OpenEnd | topo::TopoNode::MacroscopicNode));

            for &start_candidate in &node_indices {
                if km0.contains_key(&start_candidate) { continue; }

                // Start BFS from here
                let mut start_track_info = None;
                for port in [topo::Port::Single, topo::Port::Trunk, topo::Port::Crossing(topo::AB::A, 0), topo::Port::Crossing(topo::AB::A, 1)] {
                    if let Some(conn) = node_connections.get(&(start_candidate, port)) {
                        start_track_info = Some((port, conn));
                        break;
                    }
                }

                if let Some((start_port, (start_track, start_trackend))) = start_track_info {
                    km0.insert(start_candidate, (1, component_offset));
                    let start_l = topo.tracks[*start_track].length;
                    let other_node_port = track_connections.get(&(*start_track, start_trackend.opposite())).unwrap();

                    let mut stack = vec![(*other_node_port, component_offset + start_l, 1)];
                    let mut max_pos = component_offset + start_l;

                    while let Some(((node, port), pos, dir)) = stack.pop() {
                        let sw_factor = if matches!(port, topo::Port::Trunk | topo::Port::Crossing(topo::AB::A, _)) { 1 } else { -1 };
                        if let Some((node_dir, existing_pos)) = km0.get(&node) {
                            if (*node_dir) * sw_factor != dir {
                                // warn instead of error?
                                continue;
                            } else { continue; }
                        }

                        km0.insert(node, (sw_factor * dir, pos));
                        if pos > max_pos { max_pos = pos; }

                        for (other_port, next_dir) in port.other_ports() {
                            let next_dir_val = dir * next_dir;
                            if let Some((track_idx, end)) = node_connections.get(&(node, other_port)) {
                                let l = topo.tracks[*track_idx].length;
                                if let Some(target) = track_connections.get(&(*track_idx, end.opposite())) {
                                    stack.push((*target, pos + (next_dir_val as f64) * l, next_dir_val));
                                }
                            }
                        }
                    }
                    component_offset = max_pos + 100.0;
                }
            }

            debug!("KM0 in mileage estimation in raiml import");
            let mut kms = km0.iter().map(|(a,(b,c))| (a.clone(), (b.clone(),ordered_float::OrderedFloat(c.clone())))).collect::<Vec<_>>();
            kms.sort();
            for x in kms {
                debug!(" {:?}", x);
            }
            debug!("num connections {}, num nodes {}, num tracks {} len km0 {}", 
                   topo.connections.len(), topo.nodes.len(), topo.tracks.len(), km0.len());

            // now we have roughly estimated mileages and have switch orientations
            // (incoming/outgoing = increasing/decreasing milage)
            // TODO add lsqr calculations with track lengths and unknown kms.

            let mut model = plot::SchematicGraph {
                nodes: Vec::new(),
                edges: Vec::new(),
                main_tracks_edges: Vec::new(),
            };

            fn to_dir(dir :isize) -> plot::Dir { 
                match dir {
                    1 => plot::Dir::Up,
                    _ => plot::Dir::Down,
                }
            }

            for (node_idx,node_type) in topo.nodes.iter().enumerate() {
                let (dir,km0) = km0[&node_idx];

                if let topo::TopoNode::Continuation = node_type { continue; }

                model.nodes.push(plot::Node {
                    name: format!("n{}", node_idx),
                    pos: km0,
                    shape: match node_type {
                        topo::TopoNode::BufferStop | 
                        topo::TopoNode::OpenEnd | 
                        topo::TopoNode::MacroscopicNode => 
                            if dir == 1 { plot::Shape::Begin } else { plot::Shape::End },
                        topo::TopoNode::Switch(topo::Side::Left) => 
                            plot::Shape::Switch(plot::Side::Left, to_dir(dir)),
                        topo::TopoNode::Switch(topo::Side::Right) => 
                            plot::Shape::Switch(plot::Side::Right, to_dir(dir)),
                        topo::TopoNode::Crossing => plot::Shape::Crossing,
                        _ => unimplemented!(),
                    }
                });
            }

            for (i,n) in model.nodes.iter().enumerate() {
                debug!("Node {} {:?}", i, n);
            }

            let mut edges_done = HashSet::new();

            for (track_idx,_) in topo.tracks.iter().enumerate() {
                let mut na = track_connections.get(&(track_idx,topo::AB::A))
                    .ok_or(ImportState::SourceFileError(format!("Inconsistent connections.")))?;
                let mut nb = track_connections.get(&(track_idx,topo::AB::B))
                    .ok_or(ImportState::SourceFileError(format!("Inconsistent connections.")))?;

                // walk continuations
                // let track_connections :HashMap<(usize,topo::AB),(usize,topo::Port)> = 
                // let node_connections :HashMap<(usize,topo::Port),(usize,topo::AB)> = 
                fn cont_opposite(p :topo::Port) -> topo::Port {
                    match p {
                        topo::Port::ContA => topo::Port::ContB,
                        topo::Port::ContB => topo::Port::ContA,
                        x => x,
                    }
                }

                while let topo::Port::ContA | topo::Port::ContB = na.1 {
                    let (ti,tab) = node_connections.get(&(na.0, cont_opposite(na.1)))
                        .ok_or(ImportState::SourceFileError(format!("Inconsistent connections.")))?;
                    na = track_connections.get(&(*ti,tab.opposite()))
                        .ok_or(ImportState::SourceFileError(format!("Inconsistent connections.")))?;
                }
                while let topo::Port::ContA | topo::Port::ContB = nb.1 {
                    let (ti,tab) = node_connections.get(&(nb.0, cont_opposite(nb.1)))
                        .ok_or(ImportState::SourceFileError(format!("Inconsistent connections.")))?;
                    nb = track_connections.get(&(*ti,tab.opposite()))
                        .ok_or(ImportState::SourceFileError(format!("Inconsistent connections.")))?;
                }

                // swap to order pos
                if model.nodes[na.0].pos > model.nodes[nb.0].pos {
                    std::mem::swap(&mut na, &mut nb);
                }

                let convert_port = |(n,p) :(usize,topo::Port)| {
                    match p {
                        topo::Port::Trunk => plot::Port::Trunk,
                        topo::Port::Left => plot::Port::Left,
                        topo::Port::Right => plot::Port::Right,
                        topo::Port::Single => if km0[&n].0 == 1 { plot::Port::Out } else { plot::Port::In },
                        topo::Port::Crossing(topo::AB::A, 0) => plot::Port::OutLeft,
                        topo::Port::Crossing(topo::AB::B, 0) => plot::Port::InLeft,
                        topo::Port::Crossing(topo::AB::A, 1) => plot::Port::OutRight,
                        topo::Port::Crossing(topo::AB::B, 1) => plot::Port::InRight,
                        _ => unimplemented!(),
                }};

                let pa = convert_port(*na);
                let pb = convert_port(*nb);
                let a = (format!("n{}", na.0), pa);
                let b = (format!("n{}", nb.0), pb);

                let key = (a.clone(), b.clone());
                if !edges_done.contains(&key) {
                    edges_done.insert(key);
                    let mut objects = Vec::new();
                    for s in &topo.tracks[track_idx].objects.signals {
                        objects.push((plot::Symbol {
                            pos: s.pos.offset,
                            width: 0.1,
                            origin: 0.0,
                            level: 1,
                        }, RailObject::Signal(s.r#type)));
                    }
                    model.edges.push(plot::Edge { a, b, objects });
                }
            }

            Ok(model)
        }
    }
}


pub fn round_pt_tol((x,y) :(f64,f64)) -> Result<Pt,()> {
    use nalgebra_glm as glm;
    let tol = 0.05;
    if (x.round() - x).abs() > tol { return Err(()); }
    if (y.round() - y).abs() > tol { return Err(()); }
    Ok(glm::vec2(x.round() as _, (-20.0 + y.round()) as _))
}

pub fn convert_junction(plot :railplotlib::model::SchematicOutput<RailObject>) -> Result<Model, ImportState> {
    debug!("Starting conversion of railplotlib schematic output");
    
    let mut model :Model = Default::default();

    for (n,pt) in plot.nodes {
        let pt = round_pt_tol(pt)
            .map_err(|_| ImportState::PlotError(format!("Solution contains point not on grid, {:?}", pt)))?;
        use railplotlib::model::Shape;
        model.node_data.insert(pt, match n.shape {
            Shape::Begin => NDType::OpenEnd,
            Shape::End => NDType::BufferStop,
            Shape::Switch(railplotlib::model::Side::Left, _) => NDType::Sw(model::Side::Left),
            Shape::Switch(railplotlib::model::Side::Right, _) => NDType::Sw(model::Side::Right),
            Shape::Crossing => NDType::Crossing(CrossingType::Crossover),
            _ => NDType::Err,
        });
    }

    for (e,pts) in plot.lines {
        let pts = pts.into_iter().map(|x| round_pt_tol(x)).collect::<Result<Vec<_>,()>>()
            .map_err(|_| ImportState::PlotError(format!("Solution contains point not on grid")))?;
        for (p1,p2) in pts.iter().zip(pts.iter().skip(1)) {
            let segs = line_segments(*p1,*p2)
                .map_err(|_| ImportState::PlotError(format!("Line segment conversion failed")))?;
            for (p1,p2) in segs {
                model.linesegs.insert((p1,p2));
            }
        }
    }

    for (obj, pts) in plot.symbols {
        let p_res1 = round_pt_tol(pts.0);
        let p_res2 = round_pt_tol(pts.1);
        if p_res1.is_err() || p_res2.is_err() { continue; }
        let (p1, p2) = (p_res1.unwrap(), p_res2.unwrap());

        let loc = crate::document::infview::unround_coord(p1).lerp(&crate::document::infview::unround_coord(p2), 0.5);
        let tangent : Pt = p2 - p1;
        
        let mut functions = Vec::new();
        match obj {
            RailObject::Signal(t) => {
                use crate::document::objects::{Function, Object};
                match t {
                    railmlio::model::SignalType::Distant => {
                        functions.push(Function::MainSignal { has_distant: true });
                    },
                    _ => {
                        functions.push(Function::MainSignal { has_distant: false });
                    }
                }
                
                model.objects.insert(p1, Object {
                    loc,
                    tangent,
                    functions,
                });
            }
        }
    }

    Ok(model)
}

pub fn line_segments(a :Pt, b :Pt) -> Result<Vec<(Pt,Pt)>, ()> {
    use nalgebra_glm as glm;
    let mut out = Vec::new();
    let diff = b-a;
    if diff == glm::zero() { return Err(()); }
    let segs = diff.x.abs().max(diff.y.abs());
    let step_vector = glm::vec2(diff.x.signum(), diff.y.signum());
    assert_eq!(a + segs*step_vector, b);
    let mut x = a;
    for i in 0..segs {
        let y = x+step_vector;
        out.push((x,y));
        x = y;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nest_sample_import() {
        let filename = "railML/IS NEST view/2024-07-19_railML_SimpleExample_v13_NEST_railML2.5.xml".to_string();
        let (tx, rx) = std::sync::mpsc::channel();
        
        load_railml_file(filename, tx);

        let mut available_model = None;
        while let Ok(state) = rx.recv() {
            match state {
                ImportState::Available(model) => {
                    available_model = Some(model);
                    break;
                }
                ImportState::SourceFileError(e) => panic!("Source file error: {}", e),
                ImportState::PlotError(e) => panic!("Plot error: {}", e),
                _ => {}
            }
        }

        let model = available_model.expect("Model should be available");
        assert!(model.node_data.len() > 0);
        assert!(model.linesegs.len() > 0);
        println!("NEST sample import successful. Nodes: {}, Segments: {}", model.node_data.len(), model.linesegs.len());
    }
}








