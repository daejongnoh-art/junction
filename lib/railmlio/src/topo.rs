#![allow(dead_code)]

use ordered_float::OrderedFloat;
use crate::model::*;
use std::collections::HashMap;
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
}

#[derive(Debug)]
pub struct TopoTrack {
    pub objects :Objects,
    pub length: f64,
    pub offset :f64, // absolute mileage at track start if available
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

#[derive(Debug)]
pub enum TopoConvErr {
    SwitchConnectionMissing(String),
    SwitchConnectionTooMany(String),
    SwitchCourseUnknown(String),
    SwitchOrientationInvalid(String),
    UnmatchedConnection(String,String),
    TrackContinuationMismatch(String,String),
}

#[derive(Debug)]
pub struct TopoSwitchInfo {
    connref: (Id,IdRef),
    deviating_side :Side,
    switch_geometry :Side,
    dir :AB,
    pos :f64,
}

pub fn switch_info(sw :Switch) -> Result<TopoSwitchInfo,TopoConvErr> {
    match sw {
        Switch::Switch { id, pos, connections, track_continue_course, track_continue_radius, .. } => {
            match connections.as_slice() {
                &[] => Err(TopoConvErr::SwitchConnectionMissing(id)),
                &[ref connection] =>  {
                    let sw_course = connection.course
                        .or(track_continue_course.and_then(|c| c.opposite()))
                        .ok_or(TopoConvErr::SwitchCourseUnknown(id.clone()))?;

                    let deviating_side = sw_course.to_side().unwrap();
                    let switch_geometry = if connection.radius.unwrap_or(0.0) > 
                                            track_continue_radius.unwrap_or(std::f64::INFINITY) {
                        sw_course.opposite().unwrap().to_side().unwrap()
                    } else { sw_course.to_side().unwrap() };

                    Ok(
                        TopoSwitchInfo {
                            connref: (connection.id.clone(), connection.r#ref.clone()),
                            deviating_side: deviating_side,
                            switch_geometry: switch_geometry,
                            pos: pos.offset,
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
                    let sw_course = connection.course
                        .or(track_continue_course.and_then(|c| c.opposite()))
                        .ok_or(TopoConvErr::SwitchCourseUnknown(id.clone()))?;

                    let deviating_side = sw_course.to_side().unwrap();
                    let switch_geometry = if connection.radius.unwrap_or(0.0) > 
                                            track_continue_radius.unwrap_or(std::f64::INFINITY) {
                        sw_course.opposite().unwrap().to_side().unwrap()
                    } else { sw_course.to_side().unwrap() };

                    Ok(
                        TopoSwitchInfo {
                            connref: (connection.id.clone(), connection.r#ref.clone()),
                            deviating_side: deviating_side,
                            switch_geometry: switch_geometry,
                            pos: pos.offset,
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
                            connref: (connection.id.clone(), connection.r#ref.clone()),
                            deviating_side: Side::Left, // Dummy for crossing
                            switch_geometry: Side::Left, // Dummy for crossing
                            pos: pos.offset,
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

            let mut track_idx = new_track(&mut topo, TopoTrack {
                objects: Objects::empty(),
                offset: current_abs,
                length: 0.0,
            });

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
            };

            track_end(track.begin.connection, (track_idx, AB::A), &mut topo, &mut named_track_ports);
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

                named_node_ports.insert(sw_info.connref, (nd, deviating_port));

                if sw_info.dir == AB::B { std::mem::swap(&mut a_port, &mut b_port); }

                topo.connections.push(((track_idx,AB::B), (nd, a_port)));
                
                track_idx = new_track(&mut topo, TopoTrack {
                    objects: Objects::empty(),
                    offset: current_abs,
                    length: 0.0
                });
                topo.connections.push(((track_idx,AB::A), (nd, b_port)));
                current_offset = sw_info.pos;
            }

            track_end(track.end.connection, (track_idx, AB::B), &mut topo, &mut named_track_ports);
            topo.tracks[track_idx].length = track.end.pos.offset - current_offset;
            push_segment_objects(&mut topo.tracks[track_idx], current_offset, track.end.pos.offset);
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

    Ok(topo)
}

pub fn track_end(conn :TrackEndConnection, 
                 (track_idx,side) :(usize,AB),
                 topo :&mut Topological,
                 named_track_ports :&mut HashMap<(String,String),(usize,AB)>) {
    match conn {
        n @ TrackEndConnection::BufferStop | 
        n @ TrackEndConnection::OpenEnd |
        n @ TrackEndConnection::MacroscopicNode(_) => {
            let nd = new_node(topo, topo_node_type(n));
            topo.connections.push(((track_idx,side),(nd, Port::Single)));
        },
        TrackEndConnection::Connection(from,to) => {
            named_track_ports.insert((from,to),(track_idx, side));
        },
    };
}

















