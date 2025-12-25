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

fn convert_topology_to_railml(topo: &Topology) -> RailML {
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

    let mut track_connections: HashMap<(usize, AB), TrackEndConnection> = HashMap::new();
    let mut track_switches: Vec<Vec<Switch>> = vec![Vec::new(); topo.tracks.len()];

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
                    let id1 = track_conn_id(&track_id(t1), e1);
                    let id2 = track_conn_id(&track_id(t2), e2);
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
                };

                let mut connections = Vec::new();
                for (idx, (track_idx, end, port)) in ordered.iter().enumerate() {
                    let tr_id = track_id(*track_idx);
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
                };

                let mut connections = Vec::new();
                for (idx, (track_idx, end, port)) in ordered.iter().enumerate() {
                    let tr_id = track_id(*track_idx);
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
        let tr_id = track_id(idx);
        let mut ids = IdCounters::default();
        let mut objects = Objects::empty();
        let mut elements = TrackElements::empty();

        for (pos, _pt, func, dir) in topo.trackobjects[idx].iter() {
            let pos = Position {
                offset: *pos,
                mileage: None,
            };
            match func {
                Function::MainSignal { has_distant, kind } => {
                    let id = next_id("sig", &tr_id, &mut ids.signal);
                    objects.signals.push(Signal {
                        id,
                        pos,
                        name: None,
                        dir: direction_from_ab(*dir),
                        sight: None,
                        r#type: signal_type_from_kind(*kind, *has_distant),
                        function: None,
                        code: None,
                        switchable: None,
                        ocp_station_ref: None,
                    });
                }
                Function::Detector => {
                    let id = next_id("tde", &tr_id, &mut ids.detector);
                    objects.train_detectors.push(TrainDetector {
                        id,
                        pos,
                        axle_counting: None,
                        direction_detection: None,
                        medium: None,
                    });
                }
                Function::TrackCircuitBorder => {
                    let id = next_id("tcb", &tr_id, &mut ids.tcb);
                    objects.track_circuit_borders.push(TrackCircuitBorder {
                        id,
                        pos,
                        insulated_rail: None,
                    });
                }
                Function::Derailer => {
                    let id = next_id("der", &tr_id, &mut ids.derailer);
                    objects.derailers.push(Derailer {
                        id,
                        pos,
                        dir: None,
                        derail_side: None,
                        code: None,
                    });
                }
                Function::TrainProtectionElement => {
                    let id = next_id("tpe", &tr_id, &mut ids.tpe);
                    objects.train_protection_elements.push(TrainProtectionElement {
                        id,
                        pos,
                        dir: None,
                        medium: None,
                        system: None,
                    });
                }
                Function::TrainProtectionGroup => {
                    let id = next_id("tpg", &tr_id, &mut ids.tpg);
                    objects
                        .train_protection_element_groups
                        .push(TrainProtectionElementGroup {
                            id,
                            element_refs: Vec::new(),
                        });
                }
                Function::Balise => {
                    let id = next_id("bal", &tr_id, &mut ids.balise);
                    objects.balises.push(Balise { id, pos, name: None });
                }
                Function::PlatformEdge => {
                    let id = next_id("pe", &tr_id, &mut ids.platform_edge);
                    elements.platform_edges.push(PlatformEdge {
                        id,
                        name: None,
                        pos,
                        dir: TrackDirection::Down,
                        side: None,
                        height: None,
                        length: None,
                    });
                }
                Function::SpeedChange => {
                    let id = next_id("sc", &tr_id, &mut ids.speed_change);
                    elements.speed_changes.push(SpeedChange {
                        id,
                        pos,
                        dir: TrackDirection::Down,
                        vmax: None,
                        signalised: None,
                    });
                }
                Function::LevelCrossing => {
                    let id = next_id("lc", &tr_id, &mut ids.level_crossing);
                    elements.level_crossings.push(LevelCrossing {
                        id,
                        pos,
                        protection: None,
                        angle: None,
                    });
                }
                Function::CrossSection => {
                    let id = next_id("cs", &tr_id, &mut ids.cross_section);
                    elements.cross_sections.push(CrossSection {
                        id,
                        name: None,
                        ocp_ref: None,
                        pos,
                        section_type: None,
                    });
                }
            }
        }

        let begin_conn = track_connections
            .remove(&(idx, AB::A))
            .unwrap_or(TrackEndConnection::OpenEnd);
        let end_conn = track_connections
            .remove(&(idx, AB::B))
            .unwrap_or(TrackEndConnection::OpenEnd);

        let begin = Node {
            id: track_begin_id(&tr_id),
            pos: Position {
                offset: 0.0,
                mileage: None,
            },
            connection: begin_conn,
        };

        let end = Node {
            id: track_end_id(&tr_id),
            pos: Position {
                offset: *len,
                mileage: None,
            },
            connection: end_conn,
        };

        tracks.push(Track {
            id: tr_id,
            code: None,
            name: None,
            description: None,
            track_type: None,
            main_dir: None,
            begin,
            end,
            switches: track_switches[idx].clone(),
            track_elements: elements,
            objects,
        });
    }

    RailML {
        metadata: None,
        infrastructure: Some(Infrastructure {
            tracks,
            track_groups: Vec::new(),
            ocps: Vec::new(),
            states: Vec::new(),
        }),
    }
}

pub fn export_railml_to_file(filename: &str, model: &Model) -> Result<(), io::Error> {
    let topo = topology::convert(model, 50.0).map_err(|_| {
        io::Error::new(io::ErrorKind::Other, "topology conversion failed")
    })?;
    let railml = convert_topology_to_railml(&topo);
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
