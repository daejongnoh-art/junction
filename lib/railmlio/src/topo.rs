#![allow(dead_code)]

use ordered_float::OrderedFloat;
use crate::model::*;
use std::collections::{HashMap, HashSet};
use log::*;


//
// For converting:
//
//
//
pub type TopoConnection = ((usize, AB), (usize,Port));

#[derive(Debug)]
pub struct Topological {
    pub tracks :Vec<TopoTrack>,
    pub nodes :Vec<TopoNode>,
    pub connections :Vec<TopoConnection>,
    pub node_coords: Vec<Option<(f64, f64)>>,
}

#[derive(Debug)]
pub struct TopoTrack {
    pub objects :Objects,
    pub track_elements :TrackElements,
    pub length: f64,
    pub offset :f64, // absolute mileage at track start if available
    pub source: TrackSource,
    pub segment_index: usize,
    pub segment_id: String,
    pub begin_id: String,
    pub end_id: String,
}

#[derive(Debug, Clone)]
pub struct TrackSource {
    pub id: Id,
    pub code: Option<String>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub track_type: Option<String>,
    pub main_dir: Option<String>,
    pub begin_id: String,
    pub end_id: String,
    pub abs_pos_begin: Option<f64>,
    pub abs_pos_end: Option<f64>,
}

pub fn segment_track_id(base: &str, segment_index: usize) -> String {
    if segment_index == 0 {
        base.to_string()
    } else {
        format!("{}-s{}", base, segment_index)
    }
}

fn segment_begin_id(base: &str, segment_index: usize, source_begin: &str) -> String {
    if segment_index == 0 {
        source_begin.to_string()
    } else {
        format!("{}-b", segment_track_id(base, segment_index))
    }
}

fn segment_end_id(base: &str, segment_index: usize) -> String {
    format!("{}-e", segment_track_id(base, segment_index))
}
#[derive(Copy,Clone,PartialEq,Eq,Hash)]
#[derive(Debug)]
pub enum AB { A, B }

impl AB {
    pub fn opposite(&self) -> AB {
        match self {
            AB::A => AB::B,
            AB::B => AB::A,
        }
    }
}

#[derive(Debug)]
#[derive(Copy,Clone,PartialEq,Eq,Hash)]
pub enum Port {
    Trunk, Left, Right,
    Crossing(AB, usize),
    Single,
    ContA, ContB,
}

impl Port {
    pub fn other_ports(&self) -> Vec<(Port,isize)> {
        match self {
            Port::Trunk => vec![(Port::Left,1), (Port::Right,1)],
            Port::Left => vec![(Port::Right,-1), (Port::Trunk,1)],
            Port::Right => vec![(Port::Left,-1), (Port::Trunk,1)],
            Port::Single => vec![],
            Port::Crossing(ab, i) => vec![(Port::Crossing(ab.opposite(), *i), 1)],
            Port::ContA => vec![(Port::ContB,1)],
            Port::ContB => vec![(Port::ContA,1)],
        }
    }
}

#[derive(Copy,Clone)]
#[derive(Debug)]
pub enum Side { Left, Right }
impl Side {
    pub fn opposite(&self) -> Self {
        match self {
            Side::Left => Side::Right,
            Side::Right => Side::Left,
        }
    }

    pub fn to_port(&self) -> Port {
        match self {
            Side::Left => Port::Left,
            Side::Right => Port::Right,
        }
    }
}

#[derive(Debug)]
pub enum TopoNode {
    BufferStop,
    OpenEnd,
    MacroscopicNode, // TODO preserve names for boundaries?
    Switch(Side),
    Crossing,
    Continuation,
}

pub fn new_node(topo :&mut Topological, node :TopoNode) -> usize {
    let idx = topo.nodes.len();
    topo.nodes.push(node);
    topo.node_coords.push(None);
    idx
}

pub fn new_track(topo :&mut Topological, track :TopoTrack) -> usize {
    let idx = topo.tracks.len();
    topo.tracks.push(track);
    idx
}

pub fn topo_node_type(n :TrackEndConnection) -> TopoNode {
    match n {
        TrackEndConnection::BufferStop => TopoNode::BufferStop,
        TrackEndConnection::OpenEnd => TopoNode::OpenEnd,
        TrackEndConnection::MacroscopicNode(_) => TopoNode::MacroscopicNode,
        _ => panic!(),
    }
}

fn parse_geo_coord(value: &str) -> Option<(f64, f64)> {
    let cleaned = value.replace(',', " ");
    let mut it = cleaned.split_whitespace();
    let x: f64 = it.next()?.parse().ok()?;
    let y: f64 = it.next()?.parse().ok()?;
    Some((x, y))
}

#[derive(Debug)]
pub enum TopoConvErr {
    SwitchConnectionMissing(String),
    SwitchConnectionTooMany(String),
    SwitchCourseUnknown(String),
    SwitchOrientationInvalid(String),
    UnmatchedConnection(String,String),
    TrackContinuationMismatch(String,String),
    TrackEndpointMissing(usize, AB),
}

#[derive(Debug)]
pub struct TopoSwitchInfo {
    connrefs: Vec<(Id, IdRef, Option<SwitchConnectionCourse>)>,
    deviating_side :Side,
    switch_geometry :Side,
    dir :AB,
    pos :f64,
    geo_coord: Option<String>,
}

pub fn switch_info(sw :Switch) -> Result<TopoSwitchInfo,TopoConvErr> {
    match sw {
        Switch::Switch { id, pos, connections, track_continue_course, track_continue_radius, .. } => {
            match connections.as_slice() {
                &[] => Err(TopoConvErr::SwitchConnectionMissing(id)),
                &[ref connection] =>  {
                    let ref_conn = connection;
                    let sw_course = ref_conn.course
                        .or(track_continue_course.and_then(|c| c.opposite()))
                        .ok_or(TopoConvErr::SwitchCourseUnknown(id.clone()))?;

                    let deviating_side = sw_course
                        .to_side()
                        .ok_or(TopoConvErr::SwitchCourseUnknown(id.clone()))?;
                    let switch_geometry = if ref_conn.radius.unwrap_or(0.0) >
                        track_continue_radius.unwrap_or(std::f64::INFINITY) {
                        sw_course
                            .opposite()
                            .and_then(|c| c.to_side())
                            .unwrap_or(deviating_side)
                    } else { deviating_side };

                    Ok(
                        TopoSwitchInfo {
                            connrefs: connections.iter()
                                .map(|conn| (conn.id.clone(), conn.r#ref.clone(), conn.course))
                                .collect(),
                            deviating_side: deviating_side,
                            switch_geometry: switch_geometry,
                            pos: pos.offset,
                            geo_coord: pos.geo_coord.clone(),
                            dir: match connection.orientation { 
                                ConnectionOrientation::Outgoing => AB::A,
                                ConnectionOrientation::Incoming => AB::B,
                                _ => { return Err(TopoConvErr::SwitchOrientationInvalid(id.clone())); },
                            },
                        }
                    )

                },
                // railML 2.5 can list both trunk and deviating connection; use the first as reference
                &[ref connection, ..] => {
                    let ref_conn = connections.iter()
                        .find(|conn| conn.course.and_then(|c| c.to_side()).is_some())
                        .unwrap_or(connection);
                    let sw_course = ref_conn.course
                        .or(track_continue_course.and_then(|c| c.opposite()))
                        .ok_or(TopoConvErr::SwitchCourseUnknown(id.clone()))?;

                    let deviating_side = sw_course
                        .to_side()
                        .ok_or(TopoConvErr::SwitchCourseUnknown(id.clone()))?;
                    let switch_geometry = if ref_conn.radius.unwrap_or(0.0) >
                        track_continue_radius.unwrap_or(std::f64::INFINITY) {
                        sw_course
                            .opposite()
                            .and_then(|c| c.to_side())
                            .unwrap_or(deviating_side)
                    } else { deviating_side };

                    Ok(
                        TopoSwitchInfo {
                            connrefs: connections.iter()
                                .map(|conn| (conn.id.clone(), conn.r#ref.clone(), conn.course))
                                .collect(),
                            deviating_side: deviating_side,
                            switch_geometry: switch_geometry,
                            pos: pos.offset,
                            geo_coord: pos.geo_coord.clone(),
                            dir: match connection.orientation { 
                                ConnectionOrientation::Outgoing => AB::A,
                                ConnectionOrientation::Incoming => AB::B,
                                _ => { return Err(TopoConvErr::SwitchOrientationInvalid(id.clone())); },
                            },
                        }
                    )
                },
            }
        },
        Switch::Crossing { id, pos, connections, .. } => {
            match connections.as_slice() {
                &[] => Err(TopoConvErr::SwitchConnectionMissing(id)),
                &[ref connection] =>  {
                    Ok(
                        TopoSwitchInfo {
                            connrefs: connections.iter()
                                .map(|conn| (conn.id.clone(), conn.r#ref.clone(), conn.course))
                                .collect(),
                            deviating_side: Side::Left, // Dummy for crossing
                            switch_geometry: Side::Left, // Dummy for crossing
                            pos: pos.offset,
                            geo_coord: pos.geo_coord.clone(),
                            dir: match connection.orientation { 
                                ConnectionOrientation::Outgoing => AB::A,
                                ConnectionOrientation::Incoming => AB::B,
                                _ => { return Err(TopoConvErr::SwitchOrientationInvalid(id.clone())); },
                            },
                        }
                    )

                },
                _ => Err(TopoConvErr::SwitchConnectionTooMany(id)),
            }
        },
    }
}

pub fn convert_railml_topo(doc :RailML) -> Result<Topological,TopoConvErr> {
    let mut topo = Topological {
        tracks: Vec::new(),
        nodes :Vec::new(),
        connections: Vec::new(),
        node_coords: Vec::new(),
    };

    let mut named_track_ports :HashMap<(String,String), (usize, AB)> = HashMap::new();
    let mut named_node_ports  :HashMap<(String,String), (usize, Port)> = HashMap::new();

    if let Some(inf) = doc.infrastructure {
        for mut track in inf.tracks {
            let mut current_offset = 0.0;
            // infer absolute start: prefer begin.absPos, else any element with absPos - pos, else 0
            let inferred_abs = track.begin.pos.mileage
                .or(track.end.pos.mileage)
                .or_else(|| track.objects.signals.iter()
                    .find_map(|s| s.pos.mileage.map(|m| m - s.pos.offset)))
                .or_else(|| track.track_elements.platform_edges.iter()
                    .find_map(|p| p.pos.mileage.map(|m| m - p.pos.offset)))
                .or_else(|| track.track_elements.speed_changes.iter()
                    .find_map(|p| p.pos.mileage.map(|m| m - p.pos.offset)))
                .or_else(|| track.track_elements.level_crossings.iter()
                    .find_map(|p| p.pos.mileage.map(|m| m - p.pos.offset)))
                .or_else(|| track.objects.train_detectors.iter()
                    .find_map(|p| p.pos.mileage.map(|m| m - p.pos.offset)))
                .or_else(|| track.objects.track_circuit_borders.iter()
                    .find_map(|p| p.pos.mileage.map(|m| m - p.pos.offset)))
                .or_else(|| track.objects.derailers.iter()
                    .find_map(|p| p.pos.mileage.map(|m| m - p.pos.offset)))
                .or_else(|| track.objects.train_protection_elements.iter()
                    .find_map(|p| p.pos.mileage.map(|m| m - p.pos.offset)))
                .unwrap_or(0.0);
            let mut current_abs = inferred_abs;

            let mut init_objects = Objects::empty();
            init_objects.train_protection_element_groups = track.objects.train_protection_element_groups.clone();
            let source = TrackSource {
                id: track.id.clone(),
                code: track.code.clone(),
                name: track.name.clone(),
                description: track.description.clone(),
                track_type: track.track_type.clone(),
                main_dir: track.main_dir.clone(),
                begin_id: track.begin.id.clone(),
                end_id: track.end.id.clone(),
                abs_pos_begin: track.begin.pos.mileage,
                abs_pos_end: track.end.pos.mileage,
            };
            let mut segment_index = 0usize;
            let mut track_idx = new_track(&mut topo, TopoTrack {
                objects: init_objects,
                track_elements: TrackElements::empty(),
                offset: current_abs,
                length: 0.0,
                source: source.clone(),
                segment_index,
                segment_id: segment_track_id(&source.id, segment_index),
                begin_id: segment_begin_id(&source.id, segment_index, &source.begin_id),
                end_id: segment_end_id(&source.id, segment_index),
            });
            let first_track_idx = track_idx;

            // prepare sorted objects for this track
            let mut sigs = track.objects.signals.clone();
            sigs.sort_by_key(|s| OrderedFloat(s.pos.offset));
            let mut bals = track.objects.balises.clone();
            bals.sort_by_key(|b| OrderedFloat(b.pos.offset));
            let mut dets = track.objects.train_detectors.clone();
            dets.sort_by_key(|d| OrderedFloat(d.pos.offset));
            let mut tcbs = track.objects.track_circuit_borders.clone();
            tcbs.sort_by_key(|d| OrderedFloat(d.pos.offset));
            let mut ders = track.objects.derailers.clone();
            ders.sort_by_key(|d| OrderedFloat(d.pos.offset));
            let mut tpes = track.objects.train_protection_elements.clone();
            tpes.sort_by_key(|e| OrderedFloat(e.pos.offset));
            let mut pes = track.track_elements.platform_edges.clone();
            pes.sort_by_key(|p| OrderedFloat(p.pos.offset));
            let mut scs = track.track_elements.speed_changes.clone();
            scs.sort_by_key(|s| OrderedFloat(s.pos.offset));
            let mut lcs = track.track_elements.level_crossings.clone();
            lcs.sort_by_key(|l| OrderedFloat(l.pos.offset));
            let mut css = track.track_elements.cross_sections.clone();
            css.sort_by_key(|c| OrderedFloat(c.pos.offset));
            let mut gms = track.track_elements.geo_mappings.clone();
            gms.sort_by_key(|g| OrderedFloat(g.pos.offset));

            let mut push_segment_objects = |seg: &mut TopoTrack, start: f64, end: f64| {
                while let Some(s) = sigs.first() {
                    if s.pos.offset <= end {
                        let mut s = sigs.remove(0);
                        s.pos.offset -= start;
                        seg.objects.signals.push(s);
                    } else { break; }
                }
                while let Some(b) = bals.first() {
                    if b.pos.offset <= end {
                        let mut b = bals.remove(0);
                        b.pos.offset -= start;
                        seg.objects.balises.push(b);
                    } else { break; }
                }
                while let Some(d) = dets.first() {
                    if d.pos.offset <= end {
                        let mut d = dets.remove(0);
                        d.pos.offset -= start;
                        seg.objects.train_detectors.push(d);
                    } else { break; }
                }
                while let Some(t) = tcbs.first() {
                    if t.pos.offset <= end {
                        let mut t = tcbs.remove(0);
                        t.pos.offset -= start;
                        seg.objects.track_circuit_borders.push(t);
                    } else { break; }
                }
                while let Some(d) = ders.first() {
                    if d.pos.offset <= end {
                        let mut d = ders.remove(0);
                        d.pos.offset -= start;
                        seg.objects.derailers.push(d);
                    } else { break; }
                }
                while let Some(e) = tpes.first() {
                    if e.pos.offset <= end {
                        let mut e = tpes.remove(0);
                        e.pos.offset -= start;
                        seg.objects.train_protection_elements.push(e);
                    } else { break; }
                }
                while let Some(p) = pes.first() {
                    if p.pos.offset <= end {
                        let mut p = pes.remove(0);
                        p.pos.offset -= start;
                        seg.track_elements.platform_edges.push(p);
                    } else { break; }
                }
                while let Some(s) = scs.first() {
                    if s.pos.offset <= end {
                        let mut s = scs.remove(0);
                        s.pos.offset -= start;
                        seg.track_elements.speed_changes.push(s);
                    } else { break; }
                }
                while let Some(l) = lcs.first() {
                    if l.pos.offset <= end {
                        let mut l = lcs.remove(0);
                        l.pos.offset -= start;
                        seg.track_elements.level_crossings.push(l);
                    } else { break; }
                }
                while let Some(c) = css.first() {
                    if c.pos.offset <= end {
                        let mut c = css.remove(0);
                        c.pos.offset -= start;
                        seg.track_elements.cross_sections.push(c);
                    } else { break; }
                }
                while let Some(g) = gms.first() {
                    if g.pos.offset <= end {
                        let mut g = gms.remove(0);
                        g.pos.offset -= start;
                        seg.track_elements.geo_mappings.push(g);
                    } else { break; }
                }
            };

            track_end(
                track.begin.connection,
                (track_idx, AB::A),
                &mut topo,
                &mut named_track_ports,
                track.begin.pos.geo_coord.clone(),
            );
            track.switches.sort_by_key(|s| match s { 
                Switch::Switch { pos, .. } | Switch::Crossing { pos, .. } => OrderedFloat(pos.offset) });
            for sw in track.switches {
                let is_crossing = matches!(sw, Switch::Crossing { .. });
                let sw_info = switch_info(sw)?;
                debug!("Switch info b. {:?}", sw_info);
                topo.tracks[track_idx].length = sw_info.pos - current_offset;
                current_abs += topo.tracks[track_idx].length;
                push_segment_objects(&mut topo.tracks[track_idx], current_offset, sw_info.pos);

                let nd = if is_crossing {
                    new_node(&mut topo, TopoNode::Crossing)
                } else {
                    new_node(&mut topo, TopoNode::Switch(sw_info.switch_geometry))
                };
                if let Some(gc) = sw_info.geo_coord.as_ref().and_then(|v| parse_geo_coord(v)) {
                    topo.node_coords[nd] = Some(gc);
                }

                let (mut a_port, mut b_port) = if is_crossing {
                    (Port::Crossing(AB::A, 0), Port::Crossing(AB::B, 0))
                } else {
                    (Port::Trunk, sw_info.deviating_side.opposite().to_port())
                };

                let deviating_port = if is_crossing {
                    Port::Crossing(sw_info.dir.opposite(), 1)
                } else {
                    sw_info.deviating_side.to_port()
                };

                if is_crossing {
                    if let Some((id, r#ref, _)) = sw_info.connrefs.first() {
                        named_node_ports.insert((id.clone(), r#ref.clone()), (nd, deviating_port));
                    }
                } else {
                    for (id, r#ref, course) in &sw_info.connrefs {
                        let port = match course {
                            Some(SwitchConnectionCourse::Straight) => Port::Trunk,
                            Some(SwitchConnectionCourse::Left) => Port::Left,
                            Some(SwitchConnectionCourse::Right) => Port::Right,
                            None => deviating_port,
                        };
                        named_node_ports.insert((id.clone(), r#ref.clone()), (nd, port));
                    }
                }

                let pos_eps = 1e-6;
                let at_begin = (sw_info.pos - current_offset).abs() <= pos_eps;
                let at_end = (track.end.pos.offset - sw_info.pos).abs() <= pos_eps;

                if at_begin || at_end {
                    if at_end {
                        break;
                    }
                    continue;
                }

                if sw_info.dir == AB::B { std::mem::swap(&mut a_port, &mut b_port); }

                topo.connections.push(((track_idx,AB::B), (nd, a_port)));
                
                segment_index += 1;
                track_idx = new_track(&mut topo, TopoTrack {
                    objects: Objects::empty(),
                    track_elements: TrackElements::empty(),
                    offset: current_abs,
                    length: 0.0,
                    source: source.clone(),
                    segment_index,
                    segment_id: segment_track_id(&source.id, segment_index),
                    begin_id: segment_begin_id(&source.id, segment_index, &source.begin_id),
                    end_id: segment_end_id(&source.id, segment_index),
                });
                topo.connections.push(((track_idx,AB::A), (nd, b_port)));
                current_offset = sw_info.pos;
            }

            track_end(
                track.end.connection,
                (track_idx, AB::B),
                &mut topo,
                &mut named_track_ports,
                track.end.pos.geo_coord.clone(),
            );
            topo.tracks[track_idx].length = track.end.pos.offset - current_offset;
            push_segment_objects(&mut topo.tracks[track_idx], current_offset, track.end.pos.offset);
            topo.tracks[first_track_idx].begin_id = source.begin_id.clone();
            topo.tracks[track_idx].end_id = source.end_id.clone();
        }
    }

    // Match track ports with node ports.
    debug!("now matching named node track ports");
    debug!("node ports {:?}", named_node_ports);
    debug!("track ports {:?}", named_track_ports);

    for ((c,r),nd_port) in named_node_ports {
        let x = (r,c);
        let tr_port = named_track_ports.remove(&x)
            .ok_or(TopoConvErr::UnmatchedConnection(x.1,x.0))?;
        topo.connections.push((tr_port,nd_port));
    }

    // track continuations i.e. connections track->track.

    while named_track_ports.len() > 0 {
        let key = named_track_ports.keys().next().unwrap().clone();
        let ((c1,c2),(t1_idx,ab1)) = named_track_ports.remove_entry(&key).unwrap();
        if let Some((t2_idx,ab2)) = named_track_ports.remove(&(c2.clone(),c1.clone())) {
            let n = new_node(&mut topo, TopoNode::Continuation);
            topo.connections.push(((t1_idx,ab1),(n,Port::ContA)));
            topo.connections.push(((t2_idx,ab2),(n,Port::ContB)));
        } else {
            return Err(TopoConvErr::TrackContinuationMismatch(c1, c2));
        }
    }

    debug!("CONNECTIONS {:?}", topo.connections);
    for c in &topo.connections {
        debug!("{:?}", c);
    }

    // Ensure each track has both endpoints connected.
    let mut track_endpoints: HashSet<(usize, AB)> = HashSet::new();
    for (track_end, _node_end) in &topo.connections {
        track_endpoints.insert(*track_end);
    }
    for (idx, _track) in topo.tracks.iter().enumerate() {
        for side in [AB::A, AB::B] {
            if !track_endpoints.contains(&(idx, side)) {
                return Err(TopoConvErr::TrackEndpointMissing(idx, side));
            }
        }
    }

    Ok(topo)
}

pub fn track_end(conn :TrackEndConnection, 
                 (track_idx,side) :(usize,AB),
                 topo :&mut Topological,
                 named_track_ports :&mut HashMap<(String,String),(usize,AB)>,
                 geo_coord: Option<String>) {
    match conn {
        n @ TrackEndConnection::BufferStop | 
        n @ TrackEndConnection::OpenEnd |
        n @ TrackEndConnection::MacroscopicNode(_) => {
            let nd = new_node(topo, topo_node_type(n));
            if let Some(gc) = geo_coord.as_ref().and_then(|v| parse_geo_coord(v)) {
                topo.node_coords[nd] = Some(gc);
            }
            topo.connections.push(((track_idx,side),(nd, Port::Single)));
        },
        TrackEndConnection::Connection(from,to) => {
            named_track_ports.insert((from,to),(track_idx, side));
        },
    };
}

















