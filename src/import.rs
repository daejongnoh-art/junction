use std::collections::{HashMap, HashSet};
use log::*;
use matches::matches;
use const_cstr::const_cstr;
use crate::document::model::*;
use crate::document::model;
use crate::document::analysis::*;
use crate::document::infview::round_coord;
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
    auto_scale :bool,
}

impl ImportWindow {
    pub fn new(thread_pool :BackgroundJobs) -> Self {
        ImportWindow {
            open: false,
            state: ImportState::ChooseFile,
            thread: None,
            thread_pool:thread_pool,
            auto_scale: true,
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

        let mut auto_scale = self.auto_scale;
        igCheckbox(const_cstr!("Auto-scale small layouts").as_ptr(), &mut auto_scale);
        self.auto_scale = auto_scale;

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
        let auto_scale = self.auto_scale;
        self.thread_pool.execute(move || { load_railml_file(filename, tx, auto_scale); });
    }

    pub fn close(&mut self) {
        self.open = false;
        self.state = ImportState::ChooseFile;
        self.thread = None;
    }
}

pub fn load_railml_file(filename :String, tx :mpsc::Sender<ImportState>, auto_scale: bool)  {
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

    let topomodel = match railmlio::topo::convert_railml_topo(parsed.clone()) {
        Ok(m) => m,
        Err(e) => {
            println!("TOPMODEL ERR {:?}", e);
            let _ = tx.send(ImportState::SourceFileError(format!("Model conversion error: {:?}", e)));
            return;
        },
    };
    validate_topo_positions(&topomodel);
    if tx.send(ImportState::Ping).is_err() { return; }
    info!("Converted to topomodel");

    let plotmodel = match convert_railplot(&topomodel) {
        Ok(m) => m,
        Err(e) => {
            let _ = tx.send(e);
            return;
        },
    };
    if tx.send(ImportState::Ping).is_err() { return; }
    info!("Converted to plotmodel");

    let new_solver = || railplotlib::solvers::LevelsSatSolver {
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
    let mut solver = new_solver();
    let fallback_plot = simple_layout_from(&plotmodel);

    let (mut plot, used_geo) = if let Some(plot) = layout_from_geocoord(&plotmodel, &topomodel) {
        info!("Using geoCoord-based layout");
        (plot, true)
    } else {
        info!("Starting solver");
        info!("plot model {:#?}", plotmodel);
        let solved = match solver.solve(plotmodel) {
            Ok(m) => m,
            Err(e) => {
                warn!("Solver failed (FromFile): {:?}, retrying Estimated", e);
                let mut solver = new_solver();
                let est_plotmodel = match convert_railplot_estimated(&topomodel) {
                    Ok(m) => m,
                    Err(err) => {
                        let _ = tx.send(err);
                        return;
                    },
                };
                let fallback = simple_layout_from(&est_plotmodel);
                match solver.solve(est_plotmodel) {
                    Ok(m2) => m2,
                    Err(e2) => {
                        warn!("Solver failed (Estimated): {:?}, using simple layout fallback", e2);
                        match convert_junction(fallback, auto_scale) {
                            Ok((m, _)) => {
                                let _ = tx.send(ImportState::Available(m));
                            },
                            Err(err) => { let _ = tx.send(err); }
                        }
                        return;
                    },
                }
            },
        };
        (solved, false)
    };
    let y_min = plot.nodes.iter().map(|(_,pt)| pt.1).fold(f64::INFINITY, f64::min);
    let y_max = plot.nodes.iter().map(|(_,pt)| pt.1).fold(f64::NEG_INFINITY, f64::max);
    let y_range = y_max - y_min;
    let has_switch = plot.nodes.iter().any(|(n,_)| matches!(n.shape,
        railplotlib::model::Shape::Switch(_,_) | railplotlib::model::Shape::Crossing));
    if has_switch && y_range < 0.5 {
        warn!("Solver output is degenerate (flat); using fallback layout");
        plot = fallback_plot;
    }
    if tx.send(ImportState::Ping).is_err() { return; }

    info!("Found model");
    let (mut model, track_segments) = match convert_junction(plot, auto_scale && !used_geo) {
        Ok(result) => result,
        Err(e) => {
            let _ = tx.send(e);
            return;
        },
    };
    model.railml_metadata = parsed.metadata.clone();
    if let Some(inf) = parsed.infrastructure.as_ref() {
        model.railml_track_groups = inf.track_groups.clone();
        model.railml_ocps = inf.ocps.clone();
        model.railml_states = inf.states.clone();
    }
    model.railml_tracks = build_railml_tracks(&topomodel, track_segments);
    if let Some(rs) = parsed.rollingstock.as_ref() {
        for v in &rs.vehicles {
            let mut vehicle = Vehicle::default();
            vehicle.name = v.name.clone().unwrap_or_else(|| v.id.clone());
            if let Some(length) = v.length {
                vehicle.length = length as f32;
            }
            if let Some(speed) = v.speed {
                vehicle.max_vel = speed as f32;
            }
            model.vehicles.insert(vehicle);
        }
    }

    info!("Model available");
    let _ = tx.send(ImportState::Available(model));
}


#[derive(Debug, Clone)]
pub enum RailObject {
    Info(crate::document::model::RailMLObjectInfo),
}

fn validate_topo_positions(topo: &railmlio::topo::Topological) {
    let eps = 1e-6;
    let mut issues = 0usize;
    for (idx, track) in topo.tracks.iter().enumerate() {
        if track.length < -eps {
            warn!("Track {} has negative length {}", idx, track.length);
            issues += 1;
        }
        let len = track.length.max(0.0);
        let mut check = |name: &str, offset: f64| {
            if offset < -eps || offset > len + eps {
                warn!("Track {} {} offset out of range: {} (len {})", idx, name, offset, len);
                issues += 1;
            }
        };
        for s in &track.objects.signals { check("signal", s.pos.offset); }
        for b in &track.objects.balises { check("balise", b.pos.offset); }
        for d in &track.objects.train_detectors { check("detector", d.pos.offset); }
        for d in &track.objects.track_circuit_borders { check("tcb", d.pos.offset); }
        for d in &track.objects.derailers { check("derailer", d.pos.offset); }
        for e in &track.objects.train_protection_elements { check("tpe", e.pos.offset); }
        for p in &track.track_elements.platform_edges { check("platform", p.pos.offset); }
        for s in &track.track_elements.speed_changes { check("speed", s.pos.offset); }
        for l in &track.track_elements.level_crossings { check("level_crossing", l.pos.offset); }
        for c in &track.track_elements.cross_sections { check("cross_section", c.pos.offset); }
        for g in &track.track_elements.geo_mappings { check("geo_mapping", g.pos.offset); }
    }
    if issues > 0 {
        warn!("Topological position validation reported {} issues", issues);
    }
}

pub fn convert_railplot(topo :&railmlio::topo::Topological) 
    -> Result<railplotlib::model::SchematicGraph<RailObject>, ImportState> {
    convert_railplot_with_method(topo, false)
}

pub fn convert_railplot_estimated(topo :&railmlio::topo::Topological) 
    -> Result<railplotlib::model::SchematicGraph<RailObject>, ImportState> {
    convert_railplot_with_method(topo, true)
}

pub fn convert_railplot_with_method(topo :&railmlio::topo::Topological, force_estimated: bool) 
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

    // prefer absolute positions when present; fall back otherwise, unless force_estimated
    let has_abs = topo.tracks.iter().any(|t| t.offset != 0.0 || t.length > 0.0);
    let method = if force_estimated { MileageMethod::Estimated } else if has_abs { MileageMethod::FromFile } else { MileageMethod::Estimated };

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
                            pos: pos_a + s.pos.offset,
                            width: 0.1,
                            origin: 0.0,
                            level: 1,
                        }, RailObject::Info(crate::document::model::RailMLObjectInfo::Signal {
                            id: s.id.clone(),
                            sight: s.sight,
                            r#type: s.r#type,
                            function: s.function,
                            code: s.code.clone(),
                            switchable: s.switchable,
                            ocp_station_ref: s.ocp_station_ref.clone(),
                            dir: s.dir,
                        })));
                    }
                    for d in &topo.tracks[track_idx].objects.train_detectors {
                        objects.push((plot::Symbol {
                            pos: pos_a + d.pos.offset,
                            width: 0.1,
                            origin: 0.0,
                            level: 1,
                        }, RailObject::Info(crate::document::model::RailMLObjectInfo::TrainDetector {
                            id: d.id.clone(),
                            axle_counting: d.axle_counting,
                            direction_detection: d.direction_detection,
                            medium: d.medium.clone(),
                        })));
                    }
                    for d in &topo.tracks[track_idx].objects.track_circuit_borders {
                        objects.push((plot::Symbol {
                            pos: pos_a + d.pos.offset,
                            width: 0.1,
                            origin: 0.0,
                            level: 1,
                        }, RailObject::Info(crate::document::model::RailMLObjectInfo::TrackCircuitBorder {
                            id: d.id.clone(),
                            insulated_rail: d.insulated_rail.clone(),
                        })));
                    }
                    for d in &topo.tracks[track_idx].objects.derailers {
                        objects.push((plot::Symbol {
                            pos: pos_a + d.pos.offset,
                            width: 0.1,
                            origin: 0.0,
                            level: 1,
                        }, RailObject::Info(crate::document::model::RailMLObjectInfo::Derailer {
                            id: d.id.clone(),
                            dir: d.dir,
                            derail_side: d.derail_side.clone(),
                            code: d.code.clone(),
                        })));
                    }
                    for e in &topo.tracks[track_idx].objects.train_protection_elements {
                        objects.push((plot::Symbol {
                            pos: pos_a + e.pos.offset,
                            width: 0.1,
                            origin: 0.0,
                            level: 1,
                        }, RailObject::Info(crate::document::model::RailMLObjectInfo::TrainProtectionElement {
                            id: e.id.clone(),
                            dir: e.dir,
                            medium: e.medium.clone(),
                            system: e.system.clone(),
                        })));
                    }
                    for p in &topo.tracks[track_idx].track_elements.platform_edges {
                        objects.push((plot::Symbol {
                            pos: pos_a + p.pos.offset,
                            width: 0.1,
                            origin: 0.0,
                            level: 1,
                        }, RailObject::Info(crate::document::model::RailMLObjectInfo::PlatformEdge {
                            id: p.id.clone(),
                            name: p.name.clone(),
                            dir: p.dir,
                            side: p.side.clone(),
                            height: p.height,
                            length: p.length,
                        })));
                    }
                    for s in &topo.tracks[track_idx].track_elements.speed_changes {
                        objects.push((plot::Symbol {
                            pos: pos_a + s.pos.offset,
                            width: 0.1,
                            origin: 0.0,
                            level: 1,
                        }, RailObject::Info(crate::document::model::RailMLObjectInfo::SpeedChange {
                            id: s.id.clone(),
                            dir: s.dir,
                            vmax: s.vmax.clone(),
                            signalised: s.signalised,
                        })));
                    }
                    for l in &topo.tracks[track_idx].track_elements.level_crossings {
                        objects.push((plot::Symbol {
                            pos: pos_a + l.pos.offset,
                            width: 0.1,
                            origin: 0.0,
                            level: 1,
                        }, RailObject::Info(crate::document::model::RailMLObjectInfo::LevelCrossing {
                            id: l.id.clone(),
                            protection: l.protection.clone(),
                            angle: l.angle,
                        })));
                    }
                    for c in &topo.tracks[track_idx].track_elements.cross_sections {
                        objects.push((plot::Symbol {
                            pos: pos_a + c.pos.offset,
                            width: 0.1,
                            origin: 0.0,
                            level: 1,
                        }, RailObject::Info(crate::document::model::RailMLObjectInfo::CrossSection {
                            id: c.id.clone(),
                            name: c.name.clone(),
                            ocp_ref: c.ocp_ref.clone(),
                            section_type: c.section_type.clone(),
                        })));
                    }
                    for b in &topo.tracks[track_idx].objects.balises {
                        objects.push((plot::Symbol {
                            pos: pos_a + b.pos.offset,
                            width: 0.1,
                            origin: 0.0,
                            level: 1,
                        }, RailObject::Info(crate::document::model::RailMLObjectInfo::Balise {
                            id: b.id.clone(),
                            name: b.name.clone(),
                        })));
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

            let mut node_pos_map: HashMap<String, f64> = HashMap::new();
            for n in &model.nodes {
                node_pos_map.insert(n.name.clone(), n.pos);
            }

            let mut edges_done = HashSet::new();
            let mut track_start_pos: Vec<Option<f64>> = vec![None; topo.tracks.len()];
            for (track_idx, _) in topo.tracks.iter().enumerate() {
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
                let pa = node_pos_map.get(&format!("n{}", na.0)).cloned().unwrap_or(0.0);
                let pb = node_pos_map.get(&format!("n{}", nb.0)).cloned().unwrap_or(0.0);
                track_start_pos[track_idx] = Some(pa.min(pb));
            }

            let mut element_pos: HashMap<String, (usize, f64)> = HashMap::new();
            for (track_idx, track) in topo.tracks.iter().enumerate() {
                if let Some(start) = track_start_pos[track_idx] {
                    for e in &track.objects.train_protection_elements {
                        element_pos.entry(e.id.clone())
                            .or_insert((track_idx, start + e.pos.offset));
                    }
                }
            }
            let mut group_by_track: HashMap<usize, Vec<f64>> = HashMap::new();
            for track in topo.tracks.iter() {
                for g in &track.objects.train_protection_element_groups {
                    for r in &g.element_refs {
                        if let Some((tidx, pos)) = element_pos.get(r) {
                            group_by_track.entry(*tidx).or_default().push(*pos);
                        }
                    }
                }
            }
            let mut element_pos: HashMap<String, (usize, f64)> = HashMap::new();
            for (track_idx, track) in topo.tracks.iter().enumerate() {
                for e in &track.objects.train_protection_elements {
                    element_pos.entry(e.id.clone())
                        .or_insert((track_idx, track.offset + e.pos.offset));
                }
            }
            let mut group_by_track: HashMap<usize, Vec<f64>> = HashMap::new();
            for track in topo.tracks.iter() {
                for g in &track.objects.train_protection_element_groups {
                    for r in &g.element_refs {
                        if let Some((tidx, pos)) = element_pos.get(r) {
                            group_by_track.entry(*tidx).or_default().push(*pos);
                        }
                    }
                }
            }

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
                    let pos_a = node_pos_map.get(&a.0).cloned().unwrap_or(0.0);
                    for s in &topo.tracks[track_idx].objects.signals {
                        objects.push((plot::Symbol {
                            pos: pos_a + s.pos.offset,
                            width: 0.1,
                            origin: 0.0,
                            level: 1,
                        }, RailObject::Info(crate::document::model::RailMLObjectInfo::Signal {
                            id: s.id.clone(),
                            sight: s.sight,
                            r#type: s.r#type,
                            function: s.function,
                            code: s.code.clone(),
                            switchable: s.switchable,
                            ocp_station_ref: s.ocp_station_ref.clone(),
                            dir: s.dir,
                        })));
                    }
                    for d in &topo.tracks[track_idx].objects.train_detectors {
                        objects.push((plot::Symbol {
                            pos: pos_a + d.pos.offset,
                            width: 0.1,
                            origin: 0.0,
                            level: 1,
                        }, RailObject::Info(crate::document::model::RailMLObjectInfo::TrainDetector {
                            id: d.id.clone(),
                            axle_counting: d.axle_counting,
                            direction_detection: d.direction_detection,
                            medium: d.medium.clone(),
                        })));
                    }
                    for d in &topo.tracks[track_idx].objects.track_circuit_borders {
                        objects.push((plot::Symbol {
                            pos: pos_a + d.pos.offset,
                            width: 0.1,
                            origin: 0.0,
                            level: 1,
                        }, RailObject::Info(crate::document::model::RailMLObjectInfo::TrackCircuitBorder {
                            id: d.id.clone(),
                            insulated_rail: d.insulated_rail.clone(),
                        })));
                    }
                    for d in &topo.tracks[track_idx].objects.derailers {
                        objects.push((plot::Symbol {
                            pos: pos_a + d.pos.offset,
                            width: 0.1,
                            origin: 0.0,
                            level: 1,
                        }, RailObject::Info(crate::document::model::RailMLObjectInfo::Derailer {
                            id: d.id.clone(),
                            dir: d.dir,
                            derail_side: d.derail_side.clone(),
                            code: d.code.clone(),
                        })));
                    }
                    for e in &topo.tracks[track_idx].objects.train_protection_elements {
                        objects.push((plot::Symbol {
                            pos: pos_a + e.pos.offset,
                            width: 0.1,
                            origin: 0.0,
                            level: 1,
                        }, RailObject::Info(crate::document::model::RailMLObjectInfo::TrainProtectionElement {
                            id: e.id.clone(),
                            dir: e.dir,
                            medium: e.medium.clone(),
                            system: e.system.clone(),
                        })));
                    }
                    if let Some(group_positions) = group_by_track.get(&track_idx) {
                        for (idx, pos) in group_positions.iter().enumerate() {
                            let info = topo.tracks[track_idx]
                                .objects
                                .train_protection_element_groups
                                .get(idx)
                                .map(|g| crate::document::model::RailMLObjectInfo::TrainProtectionElementGroup {
                                    id: g.id.clone(),
                                    element_refs: g.element_refs.clone(),
                                });
                            if let Some(info) = info {
                                objects.push((plot::Symbol {
                                    pos: *pos,
                                    width: 0.1,
                                    origin: 0.0,
                                    level: 1,
                                }, RailObject::Info(info)));
                            }
                        }
                    }
                    for p in &topo.tracks[track_idx].track_elements.platform_edges {
                        objects.push((plot::Symbol {
                            pos: pos_a + p.pos.offset,
                            width: 0.1,
                            origin: 0.0,
                            level: 1,
                        }, RailObject::Info(crate::document::model::RailMLObjectInfo::PlatformEdge {
                            id: p.id.clone(),
                            name: p.name.clone(),
                            dir: p.dir,
                            side: p.side.clone(),
                            height: p.height,
                            length: p.length,
                        })));
                    }
                    for s in &topo.tracks[track_idx].track_elements.speed_changes {
                        objects.push((plot::Symbol {
                            pos: pos_a + s.pos.offset,
                            width: 0.1,
                            origin: 0.0,
                            level: 1,
                        }, RailObject::Info(crate::document::model::RailMLObjectInfo::SpeedChange {
                            id: s.id.clone(),
                            dir: s.dir,
                            vmax: s.vmax.clone(),
                            signalised: s.signalised,
                        })));
                    }
                    for l in &topo.tracks[track_idx].track_elements.level_crossings {
                        objects.push((plot::Symbol {
                            pos: pos_a + l.pos.offset,
                            width: 0.1,
                            origin: 0.0,
                            level: 1,
                        }, RailObject::Info(crate::document::model::RailMLObjectInfo::LevelCrossing {
                            id: l.id.clone(),
                            protection: l.protection.clone(),
                            angle: l.angle,
                        })));
                    }
                    for c in &topo.tracks[track_idx].track_elements.cross_sections {
                        objects.push((plot::Symbol {
                            pos: pos_a + c.pos.offset,
                            width: 0.1,
                            origin: 0.0,
                            level: 1,
                        }, RailObject::Info(crate::document::model::RailMLObjectInfo::CrossSection {
                            id: c.id.clone(),
                            name: c.name.clone(),
                            ocp_ref: c.ocp_ref.clone(),
                            section_type: c.section_type.clone(),
                        })));
                    }
                    for b in &topo.tracks[track_idx].objects.balises {
                        objects.push((plot::Symbol {
                            pos: pos_a + b.pos.offset,
                            width: 0.1,
                            origin: 0.0,
                            level: 1,
                        }, RailObject::Info(crate::document::model::RailMLObjectInfo::Balise {
                            id: b.id.clone(),
                            name: b.name.clone(),
                        })));
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
    // Accept solver output that is close (within tol) to integer grid and snap it.
    let tol = 0.6;
    if (x.round() - x).abs() > tol { return Err(()); }
    if (y.round() - y).abs() > tol { return Err(()); }
    Ok(glm::vec2(x.round() as _, y.round() as _))
}

fn build_track_segments(plot: &railplotlib::model::SchematicOutput<RailObject>) -> Result<Vec<Vec<(Pt,Pt)>>, ImportState> {
    let mut track_segments = Vec::new();
    for (_e, pts) in &plot.lines {
        let pts = pts
            .iter()
            .map(|x| round_pt_tol(*x))
            .collect::<Result<Vec<_>, ()>>()
            .map_err(|_| ImportState::PlotError("Solution contains point not on grid".to_string()))?;
        let mut segs = Vec::new();
        for (p1, p2) in pts.iter().zip(pts.iter().skip(1)) {
            let segs_raw = line_segments(*p1, *p2).or_else(|_| manhattan_segments(*p1, *p2));
            let segs_raw = segs_raw.unwrap_or_default();
            for (mut a, mut b) in segs_raw {
                if a > b { std::mem::swap(&mut a, &mut b); }
                segs.push((a, b));
            }
        }
        track_segments.push(segs);
    }
    Ok(track_segments)
}

fn build_railml_tracks(
    topo: &railmlio::topo::Topological,
    track_segments: Vec<Vec<(Pt,Pt)>>,
) -> Vec<crate::document::model::RailMLTrackInfo> {
    topo.tracks
        .iter()
        .enumerate()
        .map(|(idx, track)| {
            let has_abs = track.source.abs_pos_begin.is_some() || track.source.abs_pos_end.is_some();
            let abs_pos_begin = if has_abs { Some(track.offset) } else { None };
            let abs_pos_end = abs_pos_begin.map(|v| v + track.length);
            crate::document::model::RailMLTrackInfo {
                id: track.segment_id.clone(),
                code: track.source.code.clone(),
                name: track.source.name.clone(),
                description: track.source.description.clone(),
                track_type: track.source.track_type.clone(),
                main_dir: track.source.main_dir.clone(),
                begin_id: track.begin_id.clone(),
                end_id: track.end_id.clone(),
                abs_pos_begin,
                abs_pos_end,
                segments: track_segments.get(idx).cloned().unwrap_or_default(),
            }
        })
        .collect()
}

pub fn convert_junction(plot :railplotlib::model::SchematicOutput<RailObject>, auto_scale: bool) -> Result<(Model, Vec<Vec<(Pt,Pt)>>), ImportState> {
    debug!("Starting conversion of railplotlib schematic output");

    // Heuristic scaling: scale up tiny outputs and scale down huge outputs to keep grid reasonable.
    let mut plot = plot;
    if auto_scale {
        use std::cmp::Ordering;
        let max_pos = plot.nodes.iter()
            .map(|(_,pt)| pt.0.abs().max(pt.1.abs()))
            .max_by(|a,b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
        if let Some(max_pos) = max_pos {
            let scale: f64 = if max_pos > 0.0 && max_pos < 50.0 {
                50.0 / max_pos
            } else if max_pos > 500.0 {
                500.0 / max_pos
            } else {
                1.0
            };
            if (scale - 1.0).abs() > f64::EPSILON {
                debug!("Scaling plot output by factor {}", scale);
                for (_n, pt) in plot.nodes.iter_mut() {
                    pt.0 *= scale;
                    pt.1 *= scale;
                }
                for (_e,pts) in plot.lines.iter_mut() {
                    for p in pts.iter_mut() {
                        p.0 *= scale;
                        p.1 *= scale;
                    }
                }
                for (_obj, sym) in plot.symbols.iter_mut() {
                    sym.0.0 *= scale;
                    sym.0.1 *= scale;
                    sym.1.0 *= scale;
                    sym.1.1 *= scale;
                }
            }
        }
    }

    let track_segments = build_track_segments(&plot)?;
    let mut model :Model = Default::default();

    for (n,pt) in plot.nodes {
        let pt = round_pt_tol(pt)
            .map_err(|_| ImportState::PlotError(format!("Solution contains point not on grid, {:?}", pt)))?;
        use railplotlib::model::Shape;
        let nd = match n.shape {
            Shape::Begin => Some(NDType::OpenEnd),
            Shape::End => Some(NDType::BufferStop),
            Shape::Crossing => Some(NDType::Crossing(CrossingType::Crossover)),
            Shape::Switch(_, _) => None,
            _ => Some(NDType::Err),
        };
        if let Some(nd) = nd {
            model.node_data.insert(pt, nd);
        }
    }

    let mut plot_segments: Vec<((f64, f64), (f64, f64))> = Vec::new();
    for (_e, pts) in &plot.lines {
        for (p1, p2) in pts.iter().zip(pts.iter().skip(1)) {
            plot_segments.push((*p1, *p2));
        }
    }

    for (e,pts) in plot.lines {
        let pts = pts.into_iter().map(|x| round_pt_tol(x)).collect::<Result<Vec<_>,()>>()
            .map_err(|_| ImportState::PlotError(format!("Solution contains point not on grid")))?;
        for (p1,p2) in pts.iter().zip(pts.iter().skip(1)) {
            let segs = line_segments(*p1,*p2).or_else(|_| manhattan_segments(*p1,*p2));
            let segs = segs.unwrap_or_default();
            for (mut a,mut b) in segs {
                // Normalize direction: sort endpoints to avoid duplicate/overlap assertions.
                if a > b { std::mem::swap(&mut a,&mut b); }
                model.linesegs.insert((a,b));
            }
        }
    }

    let to_grid_key = |pt: (f64, f64)| {
        nalgebra_glm::vec2(pt.0.round() as i32, (pt.1.round() as i32 - 20))
    };

    let find_free_key = |base: Pt, objects: &im::HashMap<PtA, crate::document::objects::Object>| {
        if !objects.contains_key(&base) {
            return base;
        }
        for radius in 1i32..=3i32 {
            for dx in -radius..=radius {
                for dy in -radius..=radius {
                    if dx.abs() != radius && dy.abs() != radius {
                        continue;
                    }
                    let cand = nalgebra_glm::vec2(base.x + dx, base.y + dy);
                    if !objects.contains_key(&cand) {
                        return cand;
                    }
                }
            }
        }
        base
    };

    for (obj, pts) in plot.symbols {
        let p1 = round_pt_tol(pts.0).unwrap_or_else(|_| to_grid_key(pts.0));

        let mut best_tangent: Option<Pt> = None;
        let mut best_dist = f64::INFINITY;
        for (a, b) in &plot_segments {
            let (x0, y0) = (a.0, a.1);
            let (x1, y1) = (b.0, b.1);
            let (px, py) = (pts.0 .0, pts.0 .1);
            let dx = x1 - x0;
            let dy = y1 - y0;
            let len2 = dx * dx + dy * dy;
            if len2 < f64::EPSILON { continue; }
            let t = ((px - x0) * dx + (py - y0) * dy) / len2;
            let t = t.max(0.0).min(1.0);
            let projx = x0 + t * dx;
            let projy = y0 + t * dy;
            let dist = (px - projx) * (px - projx) + (py - projy) * (py - projy);
            if dist < best_dist {
                best_dist = dist;
                let adx = dx.abs();
                let ady = dy.abs();
                let tangent = if adx >= ady {
                    nalgebra_glm::vec2(dx.signum() as i32, if adx == ady { dy.signum() as i32 } else { 0 })
                } else {
                    nalgebra_glm::vec2(if adx == ady { dx.signum() as i32 } else { 0 }, dy.signum() as i32)
                };
                best_tangent = Some(tangent);
            }
        }

        let tvec = best_tangent.unwrap_or_else(|| {
            nalgebra_glm::vec2((pts.1 .0).signum() as i32, (pts.1 .1).signum() as i32)
        });
        let loc = nalgebra_glm::vec2(pts.0 .0 as f32, pts.0 .1 as f32);
        let tangent: Pt = if tvec == nalgebra_glm::zero() { nalgebra_glm::vec2(1, 0) } else { tvec };
        
        let mut functions = Vec::new();
        let mut signal_dir: Option<railmlio::model::TrackDirection> = None;
        let info = match obj {
            RailObject::Info(info) => info,
        };
        let mut tangent = tangent;
        match &info {
            crate::document::model::RailMLObjectInfo::Signal { r#type: t, dir, .. } => {
                use crate::document::objects::Function;
                let kind = match t {
                    railmlio::model::SignalType::Main => crate::document::objects::SignalKind::Main,
                    railmlio::model::SignalType::Distant => crate::document::objects::SignalKind::Distant,
                    railmlio::model::SignalType::Combined => crate::document::objects::SignalKind::Combined,
                    railmlio::model::SignalType::Repeater => crate::document::objects::SignalKind::Repeater,
                    railmlio::model::SignalType::Shunting => crate::document::objects::SignalKind::Shunting,
                };
                let has_distant = matches!(kind,
                    crate::document::objects::SignalKind::Combined |
                    crate::document::objects::SignalKind::Distant);
                functions.push(Function::MainSignal { has_distant, kind });
                signal_dir = Some(*dir);
            }
            crate::document::model::RailMLObjectInfo::TrainDetector { .. } => {
                functions.push(crate::document::objects::Function::Detector);
            }
            crate::document::model::RailMLObjectInfo::TrackCircuitBorder { .. } => {
                functions.push(crate::document::objects::Function::TrackCircuitBorder);
            }
            crate::document::model::RailMLObjectInfo::Derailer { .. } => {
                functions.push(crate::document::objects::Function::Derailer);
            }
            crate::document::model::RailMLObjectInfo::TrainProtectionElement { .. } => {
                functions.push(crate::document::objects::Function::TrainProtectionElement);
            }
            crate::document::model::RailMLObjectInfo::TrainProtectionElementGroup { .. } => {
                functions.push(crate::document::objects::Function::TrainProtectionGroup);
            }
            crate::document::model::RailMLObjectInfo::Balise { .. } => {
                functions.push(crate::document::objects::Function::Balise);
            }
            crate::document::model::RailMLObjectInfo::PlatformEdge { .. } => {
                functions.push(crate::document::objects::Function::PlatformEdge);
            }
            crate::document::model::RailMLObjectInfo::SpeedChange { .. } => {
                functions.push(crate::document::objects::Function::SpeedChange);
            }
            crate::document::model::RailMLObjectInfo::LevelCrossing { .. } => {
                functions.push(crate::document::objects::Function::LevelCrossing);
            }
            crate::document::model::RailMLObjectInfo::CrossSection { .. } => {
                functions.push(crate::document::objects::Function::CrossSection);
            }
        }
        let mut obj = crate::document::objects::Object {
            loc,
            tangent,
            functions,
        };
        if let Some(dir) = signal_dir {
            if matches!(dir, railmlio::model::TrackDirection::Down) {
                obj.tangent = -obj.tangent;
            }
            // Preserve along-track position; offset to a consistent side based on direction.
            let tangent_f = nalgebra_glm::vec2(obj.tangent.x as f32, obj.tangent.y as f32);
            let normal = nalgebra_glm::vec2(-tangent_f.y, tangent_f.x);
            obj.loc = obj.loc + (-0.25 * normal);
        } else {
            let _ = obj.move_to(&model, obj.loc);
        }
        let base_key = round_coord(obj.loc);
        let key = find_free_key(base_key, &model.objects);
        model.objects.insert(key, obj);
        model.railml_objects.entry(key).or_insert_with(Vec::new).push(info);
    }

    Ok((model, track_segments))
}

pub fn line_segments(a :Pt, b :Pt) -> Result<Vec<(Pt,Pt)>, ()> {
    use nalgebra_glm as glm;
    let mut out = Vec::new();
    let diff = b-a;
    if diff == glm::zero() { return Err(()); }
    let segs = diff.x.abs().max(diff.y.abs());
    let step_vector = glm::vec2(diff.x.signum(), diff.y.signum());
    if a + segs*step_vector != b {
        return Err(());
    }
    let mut x = a;
    for i in 0..segs {
        let y = x+step_vector;
        out.push((x,y));
        x = y;
    }
    Ok(out)
}

/// Fallback for non 45/90 degree lines: route Manhattan style.
pub fn manhattan_segments(a: Pt, b: Pt) -> Result<Vec<(Pt,Pt)>, ()> {
    let mid1 = Pt::new(b.x, a.y);
    if mid1 == a || mid1 == b { return Err(()); }
    let mut out = Vec::new();
    out.extend(line_segments(a, mid1)?);
    out.extend(line_segments(mid1, b)?);
    Ok(out)
}

/// Simple layout fallback: straight lines between nodes, y by node index.
fn simple_layout_from(plotmodel: &railplotlib::model::SchematicGraph<RailObject>) -> railplotlib::model::SchematicOutput<RailObject> {
    use ordered_float::OrderedFloat;
    use std::collections::{BTreeMap, VecDeque};
    use railplotlib::model::Port;

    let mut node_index = HashMap::new();
    for (idx, n) in plotmodel.nodes.iter().enumerate() {
        node_index.insert(n.name.clone(), idx);
    }

    let mut adjacency: Vec<Vec<(usize, Port)>> = vec![Vec::new(); plotmodel.nodes.len()];
    for e in &plotmodel.edges {
        if let (Some(&a_idx), Some(&b_idx)) = (node_index.get(&e.a.0), node_index.get(&e.b.0)) {
            adjacency[a_idx].push((b_idx, e.a.1));
            adjacency[b_idx].push((a_idx, e.b.1));
        }
    }

    fn port_offset(port: Port) -> f64 {
        match port {
            Port::Left | Port::InLeft | Port::OutLeft => -2.0,
            Port::Right | Port::InRight | Port::OutRight => 2.0,
            _ => 0.0,
        }
    }

    let mut order: Vec<usize> = (0..plotmodel.nodes.len()).collect();
    order.sort_by(|a, b| {
        plotmodel.nodes[*a]
            .pos
            .partial_cmp(&plotmodel.nodes[*b].pos)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| plotmodel.nodes[*a].name.cmp(&plotmodel.nodes[*b].name))
    });

    let mut y_levels: Vec<Option<f64>> = vec![None; plotmodel.nodes.len()];
    for &start in &order {
        if y_levels[start].is_some() {
            continue;
        }
        y_levels[start] = Some(0.0);
        let mut queue = VecDeque::new();
        queue.push_back(start);
        while let Some(idx) = queue.pop_front() {
            let y = y_levels[idx].unwrap_or(0.0);
            for (next, port) in adjacency[idx].iter().cloned() {
                if y_levels[next].is_none() {
                    y_levels[next] = Some(y + port_offset(port));
                    queue.push_back(next);
                }
            }
        }
    }

    let mut by_pos: BTreeMap<OrderedFloat<f64>, Vec<usize>> = BTreeMap::new();
    for (idx, n) in plotmodel.nodes.iter().enumerate() {
        by_pos.entry(OrderedFloat(n.pos)).or_default().push(idx);
    }
    for (_pos, mut idxs) in by_pos {
        if idxs.len() <= 1 {
            continue;
        }
        idxs.sort_by(|a, b| y_levels[*a].unwrap_or(0.0).partial_cmp(&y_levels[*b].unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal));
        let base = y_levels[idxs[0]].unwrap_or(0.0);
        let all_same = idxs.iter().all(|i| (y_levels[*i].unwrap_or(0.0) - base).abs() < 0.1);
        if all_same {
            let count = idxs.len() as f64;
            let center = (count - 1.0) / 2.0;
            for (i, idx) in idxs.into_iter().enumerate() {
                let offset = (i as f64 - center) * 1.0;
                y_levels[idx] = Some(base + offset);
            }
        }
    }

    let mut nodes = Vec::new();
    let mut node_pos = HashMap::new();
    for (idx, n) in plotmodel.nodes.iter().enumerate() {
        let pt = (n.pos, y_levels[idx].unwrap_or(0.0));
        nodes.push((n.clone(), pt));
        node_pos.insert(n.name.clone(), pt);
    }

    let mut lines = Vec::new();
    for e in &plotmodel.edges {
        let mut a_pos = *node_pos.get(&e.a.0).unwrap_or(&(0.0, 0.0));
        let mut b_pos = *node_pos.get(&e.b.0).unwrap_or(&(0.0, 0.0));
        if b_pos.0 < a_pos.0 {
            std::mem::swap(&mut a_pos, &mut b_pos);
        }
        let mut pts = vec![a_pos];
        if (a_pos.0 - b_pos.0).abs() > f64::EPSILON && (a_pos.1 - b_pos.1).abs() > f64::EPSILON {
            pts.push((b_pos.0, a_pos.1));
        }
        pts.push(b_pos);
        lines.push((e.clone(), pts));
    }

    let mut symbols = Vec::new();
    for e in &plotmodel.edges {
        let mut a_pos = *node_pos.get(&e.a.0).unwrap_or(&(0.0, 0.0));
        let mut b_pos = *node_pos.get(&e.b.0).unwrap_or(&(0.0, 0.0));
        if b_pos.0 < a_pos.0 {
            std::mem::swap(&mut a_pos, &mut b_pos);
        }
        let dx = b_pos.0 - a_pos.0;
        let dy = b_pos.1 - a_pos.1;
        let len = (dx * dx + dy * dy).sqrt();
        let tvec = if len > f64::EPSILON {
            (dx / len, dy / len)
        } else {
            (1.0, 0.0)
        };
        for (sym, obj) in &e.objects {
            let pos = if dx.abs() > f64::EPSILON {
                let t = ((sym.pos - a_pos.0) / dx).max(0.0).min(1.0);
                (a_pos.0 + dx * t, a_pos.1)
            } else if dy.abs() > f64::EPSILON {
                let t = (sym.pos / dy.abs()).max(0.0).min(1.0);
                (a_pos.0, a_pos.1 + dy.signum() * dy.abs() * t)
            } else {
                a_pos
            };
            symbols.push((obj.clone(), (pos, tvec)));
        }
    }

    railplotlib::model::SchematicOutput { nodes, lines, symbols }
}

fn layout_from_geocoord(
    plotmodel: &railplotlib::model::SchematicGraph<RailObject>,
    topo: &railmlio::topo::Topological,
) -> Option<railplotlib::model::SchematicOutput<RailObject>> {
    fn parse_geo_coord(value: &str) -> Option<(f64, f64)> {
        let cleaned = value.replace(',', " ");
        let mut it = cleaned.split_whitespace();
        let x: f64 = it.next()?.parse().ok()?;
        let y: f64 = it.next()?.parse().ok()?;
        Some((x, y))
    }

    fn push_unique(points: &mut Vec<(f64, f64)>, pt: (f64, f64)) {
        let eps = 1e-6;
        if let Some(last) = points.last() {
            if (last.0 - pt.0).abs() <= eps && (last.1 - pt.1).abs() <= eps {
                return;
            }
        }
        points.push(pt);
    }

    fn position_on_polyline(points: &[(f64, f64)], t: f64) -> ((f64, f64), (f64, f64)) {
        if points.len() < 2 {
            return (points.get(0).copied().unwrap_or((0.0, 0.0)), (1.0, 0.0));
        }
        let mut total = 0.0;
        let mut seg_lens = Vec::new();
        for (a, b) in points.iter().zip(points.iter().skip(1)) {
            let dx = b.0 - a.0;
            let dy = b.1 - a.1;
            let len = (dx * dx + dy * dy).sqrt();
            seg_lens.push(len);
            total += len;
        }
        if total <= f64::EPSILON {
            return (points[0], (1.0, 0.0));
        }
        let mut dist = (t.max(0.0).min(1.0)) * total;
        for (idx, len) in seg_lens.iter().enumerate() {
            if *len <= f64::EPSILON {
                continue;
            }
            if dist <= *len {
                let frac = dist / *len;
                let a = points[idx];
                let b = points[idx + 1];
                let dx = b.0 - a.0;
                let dy = b.1 - a.1;
                let pos = (a.0 + dx * frac, a.1 + dy * frac);
                let tvec = (dx / *len, dy / *len);
                return (pos, tvec);
            }
            dist -= *len;
        }
        let last = points[points.len() - 1];
        let prev = points[points.len() - 2];
        let dx = last.0 - prev.0;
        let dy = last.1 - prev.1;
        let len = (dx * dx + dy * dy).sqrt();
        let tvec = if len > f64::EPSILON {
            (dx / len, dy / len)
        } else {
            (1.0, 0.0)
        };
        (last, tvec)
    }

    fn position_on_geo_mapping(
        points: &[(f64, (f64, f64))],
        pos: f64,
    ) -> ((f64, f64), (f64, f64)) {
        if points.is_empty() {
            return ((0.0, 0.0), (1.0, 0.0));
        }
        if points.len() == 1 {
            return (points[0].1, (1.0, 0.0));
        }
        let clamped = pos.max(points[0].0).min(points[points.len() - 1].0);
        for i in 0..points.len() - 1 {
            let (p0, c0) = points[i];
            let (p1, c1) = points[i + 1];
            if clamped >= p0 && clamped <= p1 {
                let denom = (p1 - p0).abs();
                let t = if denom > f64::EPSILON {
                    (clamped - p0) / (p1 - p0)
                } else {
                    0.0
                };
                let dx = c1.0 - c0.0;
                let dy = c1.1 - c0.1;
                let len = (dx * dx + dy * dy).sqrt();
                let pos = (c0.0 + dx * t, c0.1 + dy * t);
                let tvec = if len > f64::EPSILON {
                    (dx / len, dy / len)
                } else {
                    (1.0, 0.0)
                };
                return (pos, tvec);
            }
        }
        let last = points[points.len() - 1].1;
        let prev = points[points.len() - 2].1;
        let dx = last.0 - prev.0;
        let dy = last.1 - prev.1;
        let len = (dx * dx + dy * dy).sqrt();
        let tvec = if len > f64::EPSILON {
            (dx / len, dy / len)
        } else {
            (1.0, 0.0)
        };
        (last, tvec)
    }

    let mut node_index = HashMap::new();
    for (idx, n) in plotmodel.nodes.iter().enumerate() {
        node_index.insert(n.name.clone(), idx);
    }

    let mut node_pos = HashMap::new();
    for n in &plotmodel.nodes {
        let idx = n.name.strip_prefix('n')?.parse::<usize>().ok()?;
        let coord = topo.node_coords.get(idx)?.as_ref()?;
        node_pos.insert(n.name.clone(), *coord);
    }

    let mut nodes = Vec::new();
    for n in &plotmodel.nodes {
        let mut shape = n.shape;
        if let railplotlib::model::Shape::Switch(side, _) = shape {
            let node_pt = *node_pos.get(&n.name)?;
            let mut inferred = None;
            for e in &plotmodel.edges {
                let (port, other_name) = if e.a.0 == n.name {
                    (e.a.1, &e.b.0)
                } else if e.b.0 == n.name {
                    (e.b.1, &e.a.0)
                } else {
                    continue;
                };
                if matches!(port, railplotlib::model::Port::Left | railplotlib::model::Port::Right) {
                    if let Some(other_pt) = node_pos.get(other_name) {
                        let dir = if other_pt.1 < node_pt.1 {
                            railplotlib::model::Dir::Up
                        } else {
                            railplotlib::model::Dir::Down
                        };
                        inferred = Some(dir);
                        break;
                    }
                }
            }
            if let Some(dir) = inferred {
                shape = railplotlib::model::Shape::Switch(side, dir);
            }
        }
        let pt = *node_pos.get(&n.name)?;
        let mut n = n.clone();
        n.shape = shape;
        nodes.push((n, pt));
    }

    let mut track_geo = HashMap::new();
    for (idx, track) in topo.tracks.iter().enumerate() {
        let mut points = Vec::new();
        for gm in &track.track_elements.geo_mappings {
            if let Some(coord) = gm.pos.geo_coord.as_ref().and_then(|v| parse_geo_coord(v)) {
                points.push((gm.pos.offset, coord));
            }
        }
        points.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        if !points.is_empty() {
            track_geo.insert(idx, points);
        }
    }

    use railmlio::topo as topo_model;
    let track_connections: HashMap<(usize, topo_model::AB), (usize, topo_model::Port)> =
        topo.connections.iter().cloned().collect();
    let node_connections: HashMap<(usize, topo_model::Port), (usize, topo_model::AB)> = topo
        .connections
        .iter()
        .map(|(a, b)| (*b, *a))
        .collect();
    let cont_opposite = |p: topo_model::Port| match p {
        topo_model::Port::ContA => topo_model::Port::ContB,
        topo_model::Port::ContB => topo_model::Port::ContA,
        x => x,
    };
    let resolve_endpoint = |track_idx: usize, side: topo_model::AB| -> Option<usize> {
        let mut next = *track_connections.get(&(track_idx, side))?;
        loop {
            match next.1 {
                topo_model::Port::ContA | topo_model::Port::ContB => {
                    let (ti, tab) = node_connections.get(&(next.0, cont_opposite(next.1)))?;
                    next = *track_connections.get(&(*ti, tab.opposite()))?;
                }
                _ => return Some(next.0),
            }
        }
    };
    let mut edge_track_map: HashMap<(usize, usize), Vec<usize>> = HashMap::new();
    for track_idx in 0..topo.tracks.len() {
        if let (Some(a), Some(b)) = (
            resolve_endpoint(track_idx, topo_model::AB::A),
            resolve_endpoint(track_idx, topo_model::AB::B),
        ) {
            let key = if a < b { (a, b) } else { (b, a) };
            edge_track_map.entry(key).or_default().push(track_idx);
        }
    }

    let mut lines = Vec::new();
    let mut edge_lines = Vec::new();
    let mut edge_track_indices = Vec::new();
    for e in &plotmodel.edges {
        let a_pos = *node_pos.get(&e.a.0)?;
        let b_pos = *node_pos.get(&e.b.0)?;
        let a_idx = e.a.0.strip_prefix('n')?.parse::<usize>().ok()?;
        let b_idx = e.b.0.strip_prefix('n')?.parse::<usize>().ok()?;
        let key = if a_idx < b_idx { (a_idx, b_idx) } else { (b_idx, a_idx) };
        let track_idx = edge_track_map
            .get_mut(&key)
            .and_then(|list| list.pop());
        edge_track_indices.push(track_idx);

        let mut pts = Vec::new();
        push_unique(&mut pts, a_pos);
        if let Some(track_idx) = track_idx {
            if let Some(geom) = track_geo.get(&track_idx) {
                let mut coords: Vec<(f64, f64)> = geom.iter().map(|(_, coord)| *coord).collect();
                if let (Some(first), Some(last)) = (coords.first(), coords.last()) {
                    let d_first = (first.0 - a_pos.0).powi(2) + (first.1 - a_pos.1).powi(2);
                    let d_last = (last.0 - a_pos.0).powi(2) + (last.1 - a_pos.1).powi(2);
                    if d_last < d_first {
                        coords.reverse();
                    }
                }
                for coord in coords {
                    push_unique(&mut pts, coord);
                }
            }
        }
        push_unique(&mut pts, b_pos);
        edge_lines.push(pts.clone());
        lines.push((e.clone(), pts));
    }

    let mut symbols = Vec::new();
    for (edge_idx, e) in plotmodel.edges.iter().enumerate() {
        let a_idx = *node_index.get(&e.a.0)?;
        let b_idx = *node_index.get(&e.b.0)?;
        let a_m = plotmodel.nodes[a_idx].pos;
        let b_m = plotmodel.nodes[b_idx].pos;
        let denom = b_m - a_m;
        let line_pts = edge_lines.get(edge_idx)?;
        let track_idx = edge_track_indices.get(edge_idx).copied().flatten();
        for (sym, obj) in &e.objects {
            let (pos, tvec) = if let Some(track_idx) = track_idx {
                if let Some(geom) = track_geo.get(&track_idx) {
                    let track = &topo.tracks[track_idx];
                    let local_pos = (sym.pos - track.offset).max(0.0).min(track.length);
                    position_on_geo_mapping(geom, local_pos)
                } else {
                    let t = if denom.abs() > f64::EPSILON {
                        ((sym.pos - a_m) / denom).max(0.0).min(1.0)
                    } else {
                        0.0
                    };
                    position_on_polyline(line_pts, t)
                }
            } else {
                let t = if denom.abs() > f64::EPSILON {
                    ((sym.pos - a_m) / denom).max(0.0).min(1.0)
                } else {
                    0.0
                };
                position_on_polyline(line_pts, t)
            };
            symbols.push((obj.clone(), (pos, tvec)));
        }
    }

    Some(railplotlib::model::SchematicOutput { nodes, lines, symbols })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::export_railml_to_file;

    #[test]
    fn test_nest_sample_import() {
        let filename = "railML/IS NEST view/2024-07-19_railML_SimpleExample_v13_NEST_railML2.5.xml".to_string();
        let (tx, rx) = std::sync::mpsc::channel();
        
        load_railml_file(filename, tx, true);

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

    #[test]
    fn test_railml_roundtrip_export() {
        let filename = "railML/IS NEST view/2024-07-19_railML_SimpleExample_v13_NEST_railML2.5.xml".to_string();
        let (tx, rx) = std::sync::mpsc::channel();

        load_railml_file(filename, tx, true);

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
        let tmp_path = std::env::temp_dir().join(format!("junction_roundtrip_{}.railml", std::process::id()));
        export_railml_to_file(tmp_path.to_str().expect("temp path"), &model)
            .expect("export should succeed");

        let xml = std::fs::read_to_string(&tmp_path).expect("exported file should exist");
        assert!(!xml.is_empty(), "exported railML should not be empty");
        let parsed = railmlio::xml::parse_railml(&xml).expect("exported railML should parse");
        let has_tracks = parsed
            .infrastructure
            .map(|inf| !inf.tracks.is_empty())
            .unwrap_or(false);
        assert!(has_tracks, "exported railML should contain tracks");

        let _ = std::fs::remove_file(tmp_path);
    }
}








