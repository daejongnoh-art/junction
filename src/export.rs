use std::collections::HashMap;
use std::io;

use log::*;

use crate::document::model::{AB, NDType, Port};
use crate::document::objects::{Function, SignalKind};
use crate::document::topology::{self, Topology};
use crate::document::model::Model;

use railmlio::model::*;
use railmlio::write::write_railml;

#[derive(Default)]
struct IdCounters {
    signal: usize,
    detector: usize,
    tcb: usize,
    derailer: usize,
    tpe: usize,
    tpg: usize,
    balise: usize,
    platform_edge: usize,
    speed_change: usize,
    level_crossing: usize,
    cross_section: usize,
}

fn next_id(prefix: &str, track_id: &str, counter: &mut usize) -> String {
    *counter += 1;
    format!("{}{}{:02}", track_id, prefix, *counter)
}

fn track_id(idx: usize) -> String {
    format!("tr{}", idx + 1)
}

fn track_begin_id(track_id: &str) -> String {
    format!("{}tb", track_id)
}

fn track_end_id(track_id: &str) -> String {
    format!("{}te", track_id)
}

fn track_conn_id(track_id: &str, end: AB) -> String {
    let idx = match end {
        AB::A => 1,
        AB::B => 2,
    };
    format!("{}c{}", track_id, idx)
}

fn encode_i32(v: i32) -> String {
    if v < 0 {
        format!("m{}", -v)
    } else {
        v.to_string()
    }
}

fn node_id(prefix: &str, pt: crate::document::model::Pt) -> String {
    format!("{}_{}_{}", prefix, encode_i32(pt.x), encode_i32(pt.y))
}

fn fmt_coord_value(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{:.1}", v)
    } else {
        format!("{}", v)
    }
}

fn geo_coord_from_xy(x: f64, y: f64) -> String {
    format!("{} {}", fmt_coord_value(x), fmt_coord_value(y))
}

fn geo_coord_from_pt(pt: crate::document::model::Pt) -> String {
    geo_coord_from_xy(pt.x as f64, pt.y as f64)
}

fn port_order(port: Port) -> u8 {
    match port {
        Port::Trunk => 0,
        Port::Left => 1,
        Port::Right => 2,
        Port::Cross(_, _) => 3,
        Port::ContA | Port::ContB => 4,
        Port::End | Port::Err => 5,
    }
}

fn course_from_port(port: Port) -> Option<SwitchConnectionCourse> {
    match port {
        Port::Left => Some(SwitchConnectionCourse::Left),
        Port::Right => Some(SwitchConnectionCourse::Right),
        Port::Trunk => Some(SwitchConnectionCourse::Straight),
        _ => None,
    }
}

fn direction_from_ab(dir: Option<AB>) -> TrackDirection {
    match dir {
        Some(AB::A) => TrackDirection::Up,
        Some(AB::B) => TrackDirection::Down,
        None => TrackDirection::Down,
    }
}

fn signal_type_from_kind(kind: SignalKind, has_distant: bool) -> SignalType {
    match kind {
        SignalKind::Main => {
            if has_distant {
                SignalType::Combined
            } else {
                SignalType::Main
            }
        }
        SignalKind::Distant => SignalType::Distant,
        SignalKind::Combined => SignalType::Combined,
        SignalKind::Repeater => SignalType::Repeater,
        SignalKind::Shunting => SignalType::Shunting,
    }
}

fn track_end_pos(length: f64, end: AB) -> f64 {
    match end {
        AB::A => 0.0,
        AB::B => length,
    }
}

fn segment_key(segments: &[(crate::document::model::Pt, crate::document::model::Pt)]) -> String {
    let mut segs = segments.to_vec();
    segs.sort_by_key(|(a, b)| (a.x, a.y, b.x, b.y));
    let mut out = String::new();
    for (a, b) in segs {
        out.push_str(&format!("{}:{}:{}:{},", a.x, a.y, b.x, b.y));
    }
    out
}

fn info_matches_function(
    info: &crate::document::model::RailMLObjectInfo,
    func: &Function,
) -> bool {
    use crate::document::model::RailMLObjectInfo::*;
    match (info, func) {
        (Signal { .. }, Function::MainSignal { .. }) => true,
        (TrainDetector { .. }, Function::Detector) => true,
        (TrackCircuitBorder { .. }, Function::TrackCircuitBorder) => true,
        (Derailer { .. }, Function::Derailer) => true,
        (TrainProtectionElement { .. }, Function::TrainProtectionElement) => true,
        (TrainProtectionElementGroup { .. }, Function::TrainProtectionGroup) => true,
        (Balise { .. }, Function::Balise) => true,
        (PlatformEdge { .. }, Function::PlatformEdge) => true,
        (SpeedChange { .. }, Function::SpeedChange) => true,
        (LevelCrossing { .. }, Function::LevelCrossing) => true,
        (CrossSection { .. }, Function::CrossSection) => true,
        _ => false,
    }
}

fn convert_topology_to_railml(topo: &Topology, model: &Model) -> RailML {
    let mut node_map: HashMap<crate::document::model::Pt, Vec<(usize, AB, Port)>> = HashMap::new();
    let mut track_lengths = Vec::new();
    for (idx, (len, (pta, porta), (ptb, portb))) in topo.tracks.iter().enumerate() {
        track_lengths.push(*len);
        node_map
            .entry(*pta)
            .or_insert_with(Vec::new)
            .push((idx, AB::A, *porta));
        node_map
            .entry(*ptb)
            .or_insert_with(Vec::new)
            .push((idx, AB::B, *portb));
    }

    let mut track_info_by_segments = HashMap::new();
    for info in &model.railml_tracks {
        let key = segment_key(&info.segments);
        track_info_by_segments.insert(key, info);
    }
    let mut track_ids = Vec::new();
    for (idx, _) in topo.tracks.iter().enumerate() {
        let segments = topo.track_segments.get(idx).cloned().unwrap_or_default();
        let info = track_info_by_segments.get(&segment_key(&segments));
        let id = info.map(|i| i.id.clone()).unwrap_or_else(|| track_id(idx));
        track_ids.push(id);
    }

    let mut track_connections: HashMap<(usize, AB), TrackEndConnection> = HashMap::new();
    let mut track_switches: Vec<Vec<Switch>> = vec![Vec::new(); topo.tracks.len()];
    let mut track_end_pts: HashMap<(usize, AB), crate::document::model::Pt> = HashMap::new();
    for (idx, (_, (pta, _), (ptb, _))) in topo.tracks.iter().enumerate() {
        track_end_pts.insert((idx, AB::A), *pta);
        track_end_pts.insert((idx, AB::B), *ptb);
    }

    for (pt, ends) in node_map.iter() {
        let nd = topo
            .locations
            .get(pt)
            .map(|(nd, _)| *nd)
            .unwrap_or(NDType::OpenEnd);

        match nd {
            NDType::OpenEnd => {
                for (track_idx, end, _) in ends {
                    track_connections.insert((*track_idx, *end), TrackEndConnection::OpenEnd);
                }
            }
            NDType::BufferStop => {
                for (track_idx, end, _) in ends {
                    track_connections.insert((*track_idx, *end), TrackEndConnection::BufferStop);
                }
            }
            NDType::Cont => {
                if ends.len() == 2 {
                    let (t1, e1, _) = ends[0];
                    let (t2, e2, _) = ends[1];
                    let id1 = track_conn_id(&track_ids[t1], e1);
                    let id2 = track_conn_id(&track_ids[t2], e2);
                    track_connections.insert((t1, e1), TrackEndConnection::Connection(id1.clone(), id2.clone()));
                    track_connections.insert((t2, e2), TrackEndConnection::Connection(id2, id1));
                } else {
                    for (track_idx, end, _) in ends {
                        track_connections.insert((*track_idx, *end), TrackEndConnection::OpenEnd);
                    }
                }
            }
            NDType::Sw(_) => {
                let switch_id = node_id("swi", *pt);
                let mut ordered = ends.clone();
                ordered.sort_by_key(|(_, _, port)| port_order(*port));

                let host_track = ordered
                    .iter()
                    .find(|(_, _, port)| *port == Port::Trunk)
                    .map(|(idx, _, _)| *idx)
                    .unwrap_or(ordered[0].0);

                let host_end = ordered
                    .iter()
                    .find(|(idx, _, _)| *idx == host_track)
                    .map(|(_, end, _)| *end)
                    .unwrap_or(AB::A);

                let host_len = track_lengths[host_track];
                let sw_pos = Position {
                    offset: track_end_pos(host_len, host_end),
                    mileage: None,
                    geo_coord: Some(geo_coord_from_pt(*pt)),
                };

                let mut connections = Vec::new();
                for (idx, (track_idx, end, port)) in ordered.iter().enumerate() {
                    let tr_id = track_ids[*track_idx].clone();
                    let track_conn = track_conn_id(&tr_id, *end);
                    let switch_conn = format!("{}c{}", switch_id, idx + 1);
                    track_connections.insert(
                        (*track_idx, *end),
                        TrackEndConnection::Connection(track_conn.clone(), switch_conn.clone()),
                    );
                    connections.push(SwitchConnection {
                        id: switch_conn,
                        r#ref: track_conn,
                        orientation: ConnectionOrientation::Incoming,
                        course: course_from_port(*port),
                        radius: None,
                        max_speed: None,
                        passable: None,
                    });
                }

                track_switches[host_track].push(Switch::Switch {
                    id: switch_id,
                    pos: sw_pos,
                    name: None,
                    description: None,
                    length: None,
                    connections,
                    track_continue_course: Some(SwitchConnectionCourse::Straight),
                    track_continue_radius: None,
                });
            }
            NDType::Crossing(_) => {
                let switch_id = node_id("crs", *pt);
                let mut ordered = ends.clone();
                ordered.sort_by_key(|(_, _, port)| port_order(*port));

                let host_track = ordered[0].0;
                let host_end = ordered[0].1;
                let host_len = track_lengths[host_track];
                let sw_pos = Position {
                    offset: track_end_pos(host_len, host_end),
                    mileage: None,
                    geo_coord: Some(geo_coord_from_pt(*pt)),
                };

                let mut connections = Vec::new();
                for (idx, (track_idx, end, port)) in ordered.iter().enumerate() {
                    let tr_id = track_ids[*track_idx].clone();
                    let track_conn = track_conn_id(&tr_id, *end);
                    let switch_conn = format!("{}c{}", switch_id, idx + 1);
                    track_connections.insert(
                        (*track_idx, *end),
                        TrackEndConnection::Connection(track_conn.clone(), switch_conn.clone()),
                    );
                    connections.push(SwitchConnection {
                        id: switch_conn,
                        r#ref: track_conn,
                        orientation: ConnectionOrientation::Incoming,
                        course: course_from_port(*port),
                        radius: None,
                        max_speed: None,
                        passable: None,
                    });
                }

                track_switches[host_track].push(Switch::Crossing {
                    id: switch_id,
                    pos: sw_pos,
                    track_continue_course: None,
                    track_continue_radius: None,
                    normal_position: None,
                    length: None,
                    connections,
                });
            }
            _ => {
                for (track_idx, end, _) in ends {
                    track_connections.insert((*track_idx, *end), TrackEndConnection::OpenEnd);
                }
            }
        }
    }

    let mut tracks = Vec::new();

    for (idx, (len, _a, _b)) in topo.tracks.iter().enumerate() {
        let segments = topo.track_segments.get(idx).cloned().unwrap_or_default();
        let info = track_info_by_segments.get(&segment_key(&segments)).cloned();
        let (tr_id, track_code, track_name, track_desc, track_type, track_main_dir, begin_id, end_id, abs_begin, abs_end) =
            if let Some(info) = info {
                (
                    info.id.clone(),
                    info.code.clone(),
                    info.name.clone(),
                    info.description.clone(),
                    info.track_type.clone(),
                    info.main_dir.clone(),
                    info.begin_id.clone(),
                    info.end_id.clone(),
                    info.abs_pos_begin,
                    info.abs_pos_end,
                )
            } else {
                (
                    track_id(idx),
                    None,
                    None,
                    None,
                    None,
                    None,
                    track_begin_id(&track_id(idx)),
                    track_end_id(&track_id(idx)),
                    None,
                    None,
                )
            };

        let scale = if let (Some(a), Some(b)) = (abs_begin, abs_end) {
            let abs_len = (b - a).abs();
            if *len > 0.0 { abs_len / *len } else { 1.0 }
        } else {
            1.0
        };
        let scaled_len = *len * scale;

        let mut ids = IdCounters::default();
        let mut objects = Objects::empty();
        let mut elements = TrackElements::empty();

        for (pos, pt, func, dir) in topo.trackobjects[idx].iter() {
            let pos = Position {
                offset: *pos * scale,
                mileage: abs_begin.map(|v| v + *pos * scale),
                geo_coord: None,
            };
            let info = model
                .railml_objects
                .get(pt)
                .and_then(|infos| infos.iter().find(|i| info_matches_function(i, func)));
            match func {
                Function::MainSignal { has_distant, kind } => {
                    let id = info
                        .and_then(|i| match i {
                            crate::document::model::RailMLObjectInfo::Signal { id, .. } => Some(id.clone()),
                            _ => None,
                        })
                        .unwrap_or_else(|| next_id("sig", &tr_id, &mut ids.signal));
                    objects.signals.push(Signal {
                        id,
                        pos,
                        name: None,
                        dir: info
                            .and_then(|i| match i {
                                crate::document::model::RailMLObjectInfo::Signal { dir, .. } => Some(*dir),
                                _ => None,
                            })
                            .unwrap_or_else(|| direction_from_ab(*dir)),
                        sight: info
                            .and_then(|i| match i {
                                crate::document::model::RailMLObjectInfo::Signal { sight, .. } => *sight,
                                _ => None,
                            }),
                        r#type: signal_type_from_kind(*kind, *has_distant),
                        function: info
                            .and_then(|i| match i {
                                crate::document::model::RailMLObjectInfo::Signal { function, .. } => *function,
                                _ => None,
                            }),
                        code: info
                            .and_then(|i| match i {
                                crate::document::model::RailMLObjectInfo::Signal { code, .. } => code.clone(),
                                _ => None,
                            }),
                        switchable: info
                            .and_then(|i| match i {
                                crate::document::model::RailMLObjectInfo::Signal { switchable, .. } => *switchable,
                                _ => None,
                            }),
                        ocp_station_ref: info
                            .and_then(|i| match i {
                                crate::document::model::RailMLObjectInfo::Signal { ocp_station_ref, .. } => ocp_station_ref.clone(),
                                _ => None,
                            }),
                        speeds: Vec::new(),
                        etcs: None,
                    });
                }
                Function::Detector => {
                    let id = info
                        .and_then(|i| match i {
                            crate::document::model::RailMLObjectInfo::TrainDetector { id, .. } => Some(id.clone()),
                            _ => None,
                        })
                        .unwrap_or_else(|| next_id("tde", &tr_id, &mut ids.detector));
                    objects.train_detectors.push(TrainDetector {
                        id,
                        pos,
                        axle_counting: info
                            .and_then(|i| match i {
                                crate::document::model::RailMLObjectInfo::TrainDetector { axle_counting, .. } => *axle_counting,
                                _ => None,
                            }),
                        direction_detection: info
                            .and_then(|i| match i {
                                crate::document::model::RailMLObjectInfo::TrainDetector { direction_detection, .. } => *direction_detection,
                                _ => None,
                            }),
                        medium: info
                            .and_then(|i| match i {
                                crate::document::model::RailMLObjectInfo::TrainDetector { medium, .. } => medium.clone(),
                                _ => None,
                            }),
                    });
                }
                Function::TrackCircuitBorder => {
                    let id = info
                        .and_then(|i| match i {
                            crate::document::model::RailMLObjectInfo::TrackCircuitBorder { id, .. } => Some(id.clone()),
                            _ => None,
                        })
                        .unwrap_or_else(|| next_id("tcb", &tr_id, &mut ids.tcb));
                    objects.track_circuit_borders.push(TrackCircuitBorder {
                        id,
                        pos,
                        insulated_rail: info
                            .and_then(|i| match i {
                                crate::document::model::RailMLObjectInfo::TrackCircuitBorder { insulated_rail, .. } => insulated_rail.clone(),
                                _ => None,
                            }),
                    });
                }
                Function::Derailer => {
                    let id = info
                        .and_then(|i| match i {
                            crate::document::model::RailMLObjectInfo::Derailer { id, .. } => Some(id.clone()),
                            _ => None,
                        })
                        .unwrap_or_else(|| next_id("der", &tr_id, &mut ids.derailer));
                    objects.derailers.push(Derailer {
                        id,
                        pos,
                        dir: info
                            .and_then(|i| match i {
                                crate::document::model::RailMLObjectInfo::Derailer { dir, .. } => *dir,
                                _ => None,
                            }),
                        derail_side: info
                            .and_then(|i| match i {
                                crate::document::model::RailMLObjectInfo::Derailer { derail_side, .. } => derail_side.clone(),
                                _ => None,
                            }),
                        code: info
                            .and_then(|i| match i {
                                crate::document::model::RailMLObjectInfo::Derailer { code, .. } => code.clone(),
                                _ => None,
                            }),
                    });
                }
                Function::TrainProtectionElement => {
                    let id = info
                        .and_then(|i| match i {
                            crate::document::model::RailMLObjectInfo::TrainProtectionElement { id, .. } => Some(id.clone()),
                            _ => None,
                        })
                        .unwrap_or_else(|| next_id("tpe", &tr_id, &mut ids.tpe));
                    objects.train_protection_elements.push(TrainProtectionElement {
                        id,
                        pos,
                        dir: info
                            .and_then(|i| match i {
                                crate::document::model::RailMLObjectInfo::TrainProtectionElement { dir, .. } => *dir,
                                _ => None,
                            }),
                        medium: info
                            .and_then(|i| match i {
                                crate::document::model::RailMLObjectInfo::TrainProtectionElement { medium, .. } => medium.clone(),
                                _ => None,
                            }),
                        system: info
                            .and_then(|i| match i {
                                crate::document::model::RailMLObjectInfo::TrainProtectionElement { system, .. } => system.clone(),
                                _ => None,
                            }),
                    });
                }
                Function::TrainProtectionGroup => {
                    if let Some(group) = info.and_then(|i| match i {
                        crate::document::model::RailMLObjectInfo::TrainProtectionElementGroup { id, element_refs } => {
                            Some((id.clone(), element_refs.clone()))
                        }
                        _ => None,
                    }) {
                        objects
                            .train_protection_element_groups
                            .push(TrainProtectionElementGroup {
                                id: group.0,
                                element_refs: group.1,
                            });
                    } else {
                        let id = next_id("tpg", &tr_id, &mut ids.tpg);
                        objects
                            .train_protection_element_groups
                            .push(TrainProtectionElementGroup {
                                id,
                                element_refs: Vec::new(),
                            });
                    }
                }
                Function::Balise => {
                    let id = info
                        .and_then(|i| match i {
                            crate::document::model::RailMLObjectInfo::Balise { id, .. } => Some(id.clone()),
                            _ => None,
                        })
                        .unwrap_or_else(|| next_id("bal", &tr_id, &mut ids.balise));
                    let name = info
                        .and_then(|i| match i {
                            crate::document::model::RailMLObjectInfo::Balise { name, .. } => name.clone(),
                            _ => None,
                        });
                    objects.balises.push(Balise { id, pos, name });
                }
                Function::PlatformEdge => {
                    let id = info
                        .and_then(|i| match i {
                            crate::document::model::RailMLObjectInfo::PlatformEdge { id, .. } => Some(id.clone()),
                            _ => None,
                        })
                        .unwrap_or_else(|| next_id("pe", &tr_id, &mut ids.platform_edge));
                    elements.platform_edges.push(PlatformEdge {
                        id,
                        name: info
                            .and_then(|i| match i {
                                crate::document::model::RailMLObjectInfo::PlatformEdge { name, .. } => name.clone(),
                                _ => None,
                            }),
                        pos,
                        dir: info
                            .and_then(|i| match i {
                                crate::document::model::RailMLObjectInfo::PlatformEdge { dir, .. } => Some(*dir),
                                _ => None,
                            })
                            .unwrap_or(TrackDirection::Down),
                        side: info
                            .and_then(|i| match i {
                                crate::document::model::RailMLObjectInfo::PlatformEdge { side, .. } => side.clone(),
                                _ => None,
                            }),
                        height: info
                            .and_then(|i| match i {
                                crate::document::model::RailMLObjectInfo::PlatformEdge { height, .. } => *height,
                                _ => None,
                            }),
                        length: info
                            .and_then(|i| match i {
                                crate::document::model::RailMLObjectInfo::PlatformEdge { length, .. } => *length,
                                _ => None,
                            }),
                    });
                }
                Function::SpeedChange => {
                    let id = info
                        .and_then(|i| match i {
                            crate::document::model::RailMLObjectInfo::SpeedChange { id, .. } => Some(id.clone()),
                            _ => None,
                        })
                        .unwrap_or_else(|| next_id("sc", &tr_id, &mut ids.speed_change));
                    elements.speed_changes.push(SpeedChange {
                        id,
                        pos,
                        dir: info
                            .and_then(|i| match i {
                                crate::document::model::RailMLObjectInfo::SpeedChange { dir, .. } => Some(*dir),
                                _ => None,
                            })
                            .unwrap_or(TrackDirection::Down),
                        vmax: info
                            .and_then(|i| match i {
                                crate::document::model::RailMLObjectInfo::SpeedChange { vmax, .. } => vmax.clone(),
                                _ => None,
                            }),
                        signalised: info
                            .and_then(|i| match i {
                                crate::document::model::RailMLObjectInfo::SpeedChange { signalised, .. } => *signalised,
                                _ => None,
                            }),
                    });
                }
                Function::LevelCrossing => {
                    let id = info
                        .and_then(|i| match i {
                            crate::document::model::RailMLObjectInfo::LevelCrossing { id, .. } => Some(id.clone()),
                            _ => None,
                        })
                        .unwrap_or_else(|| next_id("lc", &tr_id, &mut ids.level_crossing));
                    elements.level_crossings.push(LevelCrossing {
                        id,
                        pos,
                        protection: info
                            .and_then(|i| match i {
                                crate::document::model::RailMLObjectInfo::LevelCrossing { protection, .. } => protection.clone(),
                                _ => None,
                            }),
                        angle: info
                            .and_then(|i| match i {
                                crate::document::model::RailMLObjectInfo::LevelCrossing { angle, .. } => *angle,
                                _ => None,
                            }),
                    });
                }
                Function::CrossSection => {
                    let id = info
                        .and_then(|i| match i {
                            crate::document::model::RailMLObjectInfo::CrossSection { id, .. } => Some(id.clone()),
                            _ => None,
                        })
                        .unwrap_or_else(|| next_id("cs", &tr_id, &mut ids.cross_section));
                    elements.cross_sections.push(CrossSection {
                        id,
                        name: info
                            .and_then(|i| match i {
                                crate::document::model::RailMLObjectInfo::CrossSection { name, .. } => name.clone(),
                                _ => None,
                            }),
                        ocp_ref: info
                            .and_then(|i| match i {
                                crate::document::model::RailMLObjectInfo::CrossSection { ocp_ref, .. } => ocp_ref.clone(),
                                _ => None,
                            }),
                        pos,
                        section_type: info
                            .and_then(|i| match i {
                                crate::document::model::RailMLObjectInfo::CrossSection { section_type, .. } => section_type.clone(),
                                _ => None,
                            }),
                    });
                }
            }
        }

        if let Some(lines) = topo.interval_lines.get(idx) {
            for (gm_idx, (pos, pt)) in lines.iter().enumerate() {
                let offset = pos.0 * scale;
                let mileage = abs_begin.map(|v| v + offset);
                let coord = geo_coord_from_xy(pt.x as f64, pt.y as f64);
                elements.geo_mappings.push(GeoMapping {
                    id: format!("{}gm{:02}", tr_id, gm_idx + 1),
                    pos: Position {
                        offset,
                        mileage,
                        geo_coord: Some(coord),
                    },
                    name: None,
                    code: None,
                    description: None,
                });
            }
        }

        let begin_conn = track_connections
            .remove(&(idx, AB::A))
            .unwrap_or(TrackEndConnection::OpenEnd);
        let end_conn = track_connections
            .remove(&(idx, AB::B))
            .unwrap_or(TrackEndConnection::OpenEnd);

        let begin = Node {
            id: begin_id.clone(),
            pos: Position {
                offset: 0.0,
                mileage: abs_begin,
                geo_coord: track_end_pts
                    .get(&(idx, AB::A))
                    .map(|pt| geo_coord_from_pt(*pt)),
            },
            connection: begin_conn,
        };

        let end = Node {
            id: end_id.clone(),
            pos: Position {
                offset: scaled_len,
                mileage: abs_begin.map(|v| v + scaled_len),
                geo_coord: track_end_pts
                    .get(&(idx, AB::B))
                    .map(|pt| geo_coord_from_pt(*pt)),
            },
            connection: end_conn,
        };

        tracks.push(Track {
            id: tr_id,
            code: track_code,
            name: track_name,
            description: track_desc,
            track_type: track_type,
            main_dir: track_main_dir,
            begin,
            end,
            switches: track_switches[idx].clone(),
            track_elements: elements,
            objects,
        });
    }

    RailML {
        metadata: model.railml_metadata.clone(),
        infrastructure: Some(Infrastructure {
            tracks,
            track_groups: model.railml_track_groups.clone(),
            ocps: model.railml_ocps.clone(),
            states: model.railml_states.clone(),
        }),
        rollingstock: build_rollingstock(model),
    }
}

fn build_rollingstock(model: &Model) -> Option<Rollingstock> {
    if model.vehicles.data().is_empty() {
        return None;
    }

    let vehicles = model
        .vehicles
        .data()
        .iter()
        .map(|(id, v)| Vehicle {
            id: format!("veh{}", id),
            name: Some(v.name.clone()),
            description: None,
            length: Some(v.length as f64),
            speed: Some(v.max_vel as f64),
        })
        .collect();

    Some(Rollingstock { vehicles })
}

pub fn export_railml_to_file(filename: &str, model: &Model) -> Result<(), io::Error> {
    let topo = topology::convert(model, 50.0).map_err(|_| {
        io::Error::new(io::ErrorKind::Other, "topology conversion failed")
    })?;
    let railml = convert_topology_to_railml(&topo, model);
    let xml = write_railml(&railml);
    std::fs::write(filename, xml)?;
    Ok(())
}

pub fn export_railml_interactive(model: &Model) -> Result<(), io::Error> {
    if let Some(filename) = tinyfiledialogs::save_file_dialog("Export railML to file", "") {
        info!("Exporting railML to {:?}", filename);
        export_railml_to_file(&filename, model)?;
    } else {
        info!("User cancelled railML export");
    }
    Ok(())
}
