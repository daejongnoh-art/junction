use crate::model::*;
use roxmltree as xml;
type BoxResult<T> = Result<T, Box<dyn std::error::Error>>;

pub fn parse_railml(data: &str) -> BoxResult<RailML> {
    let doc = roxmltree::Document::parse(data)?;
    parse_railml_xml(&doc.root_element())
}

pub type ByteOffset = usize;
#[derive(Debug)]
pub enum DocErr {
    ElementMissing(&'static str, ByteOffset),
    AttributeMissing(&'static str, ByteOffset),
    UnexpectedElement(String, ByteOffset),
    NumberError(ByteOffset),
    BoolError(ByteOffset),
    EnumErr(&'static str, ByteOffset),
}

fn parse_railml_xml(root: &xml::Node) -> BoxResult<RailML> {
    Ok(RailML {
        metadata: parse_metadata(root).ok(),
        infrastructure: match root.children().find(|c| c.has_tag_name("infrastructure")) {
            Some(inf) => Some(parse_infrastructure(&inf).map_err(|e| format!("{:?}", e))?),
            None => None,
        },
        rollingstock: parse_rollingstock(root).ok(),
    })
}

fn parse_metadata(root: &xml::Node) -> Result<Metadata, DocErr> {
    let md = root
        .children()
        .find(|c| c.has_tag_name("metadata"))
        .ok_or(DocErr::ElementMissing("metadata", root.range().start))?;

    let text_of = |name: &str| -> Option<String> {
        md.children()
            .find(|c| c.tag_name().name() == name)
            .and_then(|n| n.text())
            .map(|t| t.trim().to_string())
    };

    let mut organizational_units = Vec::new();
    if let Some(orgs) = md
        .children()
        .find(|c| c.tag_name().name() == "organizationalUnits")
    {
        for iu in orgs
            .children()
            .filter(|c| c.has_tag_name("infrastructureManager"))
        {
            organizational_units.push(OrganizationalUnit {
                id: iu.attribute("id").unwrap_or_default().to_string(),
                code: iu.attribute("code").map(|x| x.to_string()),
                name: iu.attribute("name").map(|x| x.to_string()),
                contact: iu.attribute("contact").map(|x| x.to_string()),
            });
        }
    }

    Ok(Metadata {
        dc_format: text_of("format"),
        dc_identifier: text_of("identifier"),
        dc_source: text_of("source"),
        dc_title: text_of("title"),
        dc_language: text_of("language"),
        dc_creator: text_of("creator"),
        dc_description: text_of("description"),
        dc_rights: text_of("rights"),
        organizational_units,
        version: md.attribute("version").map(|x| x.to_string()),
    })
}

fn parse_infrastructure(inf: &xml::Node) -> Result<Infrastructure, DocErr> {
    let mut tracks = Vec::new();
    if let Some(ts) = inf.children().find(|c| c.has_tag_name("tracks")) {
        for t in ts.children().filter(|c| c.has_tag_name("track")) {
            tracks.push(parse_track(&t)?);
        }
    }

    let mut track_groups = Vec::new();
    if let Some(tg) = inf.children().find(|c| c.has_tag_name("trackGroups")) {
        for line in tg.children().filter(|c| c.has_tag_name("line")) {
            track_groups.push(parse_track_group(&line)?);
        }
    }

    let mut ocps = Vec::new();
    if let Some(ocp_root) = inf
        .children()
        .find(|c| c.has_tag_name("operationControlPoints"))
    {
        for ocp in ocp_root.children().filter(|c| c.has_tag_name("ocp")) {
            ocps.push(parse_ocp(&ocp)?);
        }
    }

    let mut states = Vec::new();
    if let Some(state_root) = inf.children().find(|c| c.has_tag_name("states")) {
        for st in state_root.children().filter(|c| c.has_tag_name("state")) {
            states.push(parse_state(&st)?);
        }
    }

    Ok(Infrastructure {
        tracks,
        track_groups,
        ocps,
        states,
    })
}

fn parse_rollingstock(root: &xml::Node) -> Result<Rollingstock, DocErr> {
    let rs = root
        .children()
        .find(|c| c.has_tag_name("rollingstock"))
        .ok_or(DocErr::ElementMissing("rollingstock", root.range().start))?;

    let mut vehicles = Vec::new();
    if let Some(vehicles_root) = rs.children().find(|c| c.has_tag_name("vehicles")) {
        for v in vehicles_root.children().filter(|c| c.has_tag_name("vehicle")) {
            vehicles.push(parse_vehicle(&v)?);
        }
    }

    Ok(Rollingstock { vehicles })
}

fn parse_vehicle(node: &xml::Node) -> Result<Vehicle, DocErr> {
    Ok(Vehicle {
        id: node
            .attribute("id")
            .ok_or(DocErr::AttributeMissing("id", node.range().start))?
            .to_string(),
        name: node.attribute("name").map(|x| x.to_string()),
        description: node.attribute("description").map(|x| x.to_string()),
        length: node.attribute("length").and_then(|v| v.parse().ok()),
        speed: node.attribute("speed").and_then(|v| v.parse().ok()),
    })
}

fn parse_track_group(node: &xml::Node) -> Result<TrackGroup, DocErr> {
    let mut track_refs = Vec::new();
    for tr in node.children().filter(|c| c.has_tag_name("trackRef")) {
        track_refs.push(TrackRef {
            r#ref: tr
                .attribute("ref")
                .ok_or(DocErr::AttributeMissing("ref", tr.range().start))?
                .to_string(),
            sequence: tr.attribute("sequence").and_then(|s| s.parse().ok()),
        });
    }
    let mut additional_names = Vec::new();
    for an in node.children().filter(|c| c.has_tag_name("additionalName")) {
        if let Some(name) = an.attribute("name") {
            additional_names.push(AdditionalName {
                name: name.to_string(),
                lang: an.attribute("xml:lang").or_else(|| an.attribute("lang")).map(|x| x.to_string()),
                name_type: an.attribute("type").map(|x| x.to_string()),
            });
        }
    }

    Ok(TrackGroup {
        id: node
            .attribute("id")
            .ok_or(DocErr::AttributeMissing("id", node.range().start))?
            .to_string(),
        code: node.attribute("code").map(|x| x.to_string()),
        name: node.attribute("name").map(|x| x.to_string()),
        infrastructure_manager_ref: node.attribute("infrastructureManagerRef").map(|x| x.to_string()),
        line_category: node.attribute("lineCategory").map(|x| x.to_string()),
        line_type: node.attribute("type").map(|x| x.to_string()),
        track_refs,
        additional_names,
    })
}

fn parse_ocp(node: &xml::Node) -> Result<Ocp, DocErr> {
    let mut additional_names = Vec::new();
    for an in node.children().filter(|c| c.has_tag_name("additionalName")) {
        if let Some(name) = an.attribute("name") {
            additional_names.push(AdditionalName {
                name: name.to_string(),
                lang: an.attribute("xml:lang").or_else(|| an.attribute("lang")).map(|x| x.to_string()),
                name_type: an.attribute("type").map(|x| x.to_string()),
            });
        }
    }

    let prop_operational = node.children().find(|c| c.has_tag_name("propOperational")).map(|p| {
        PropOperational {
            ensures_train_sequence: p.attribute("ensuresTrainSequence").and_then(|v| v.parse::<bool>().ok()),
            order_changeable: p.attribute("orderChangeable").and_then(|v| v.parse::<bool>().ok()),
            operational_type: p.attribute("operationalType").map(|x| x.to_string()),
            traffic_type: p.attribute("trafficType").map(|x| x.to_string()),
        }
    });

    let prop_service = node.children().find(|c| c.has_tag_name("propService")).map(|p| {
        PropService {
            passenger: p.attribute("passenger").and_then(|v| v.parse::<bool>().ok()),
            service: p.attribute("service").and_then(|v| v.parse::<bool>().ok()),
            goods_siding: p.attribute("goodsSiding").and_then(|v| v.parse::<bool>().ok()),
        }
    });

    let prop_equipment = node.children().find(|c| c.has_tag_name("propEquipment")).map(|p| {
        let summary = p.children().find(|c| c.has_tag_name("summary")).map(|s| {
            PropEquipmentSummary {
                has_home_signals: s.attribute("hasHomeSignals").and_then(|v| v.parse::<bool>().ok()),
                has_starter_signals: s.attribute("hasStarterSignals").and_then(|v| v.parse::<bool>().ok()),
                has_switches: s.attribute("hasSwitches").and_then(|v| v.parse::<bool>().ok()),
                signal_box: s.attribute("signalBox").map(|x| x.to_string()),
            }
        });
        let mut track_refs = Vec::new();
        for tr in p.children().filter(|c| c.has_tag_name("trackRef")) {
            if let Some(r) = tr.attribute("ref") {
                track_refs.push(r.to_string());
            }
        }
        PropEquipment { summary, track_refs }
    });

    let designator = node.children().find(|c| c.has_tag_name("designator")).map(|d| {
        Designator {
            register: d.attribute("register").map(|x| x.to_string()),
            entry: d.attribute("entry").map(|x| x.to_string()),
        }
    });

    let geo_coord = node.children().find(|c| c.has_tag_name("geoCoord")).and_then(|g| {
        g.attribute("coord").map(|coord| GeoCoord {
            coord: coord.to_string(),
            epsg_code: g.attribute("epsgCode").map(|x| x.to_string()),
        })
    });

    Ok(Ocp {
        id: node
            .attribute("id")
            .ok_or(DocErr::AttributeMissing("id", node.range().start))?
            .to_string(),
        name: node.attribute("name").map(|x| x.to_string()),
        lang: node.attribute("xml:lang").or_else(|| node.attribute("lang")).map(|x| x.to_string()),
        r#type: node.attribute("type").map(|x| x.to_string()),
        geo_coord,
        additional_names,
        prop_operational,
        prop_equipment,
        prop_service,
        designator,
    })
}

fn parse_state(node: &xml::Node) -> Result<State, DocErr> {
    Ok(State {
        id: node
            .attribute("id")
            .ok_or(DocErr::AttributeMissing("id", node.range().start))?
            .to_string(),
        disabled: node
            .attribute("disabled")
            .map(|v| v.parse::<bool>().ok())
            .flatten(),
        status: node.attribute("status").map(|x| x.to_string()),
    })
}

fn parse_track(track: &xml::Node) -> Result<Track, DocErr> {
    let topo = track
        .children()
        .find(|c| c.has_tag_name("trackTopology"))
        .ok_or(DocErr::ElementMissing(
            "trackTopology",
            track.range().start,
        ))?;

    Ok(Track {
        id: track
            .attribute("id")
            .ok_or(DocErr::AttributeMissing("id", track.range().start))?
            .to_string(),
        name: track.attribute("name").map(|x| x.to_string()),
        code: track.attribute("code").map(|x| x.to_string()),
        description: track.attribute("description").map(|x| x.to_string()),
        track_type: track.attribute("type").map(|x| x.to_string()),
        main_dir: track.attribute("mainDir").map(|x| x.to_string()),
        begin: parse_track_node(
            &topo
                .children()
                .find(|c| c.has_tag_name("trackBegin"))
                .ok_or(DocErr::ElementMissing("trackBegin", topo.range().start))?,
        )?,
        end: parse_track_node(
            &topo
                .children()
                .find(|c| c.has_tag_name("trackEnd"))
                .ok_or(DocErr::ElementMissing("trackEnd", topo.range().start))?,
        )?,
        switches: parse_switches(&topo)?,
        track_elements: parse_track_elements(track, &topo)?,
        objects: parse_objects(track)?,
    })
}

fn parse_track_elements(track: &xml::Node, topo: &xml::Node) -> Result<TrackElements, DocErr> {
    let mut res = TrackElements::empty();
    if let Some(cs) = topo.children().find(|c| c.has_tag_name("crossSections")) {
        for c in cs.children().filter(|c| c.has_tag_name("crossSection")) {
            res.cross_sections.push(parse_cross_section(&c)?);
        }
    }
    if let Some(te) = track.children().find(|c| c.has_tag_name("trackElements")) {
        if let Some(pes) = te.children().find(|c| c.has_tag_name("platformEdges")) {
            for p in pes.children().filter(|c| c.has_tag_name("platformEdge")) {
                res.platform_edges.push(parse_platform_edge(&p)?);
            }
        }
        if let Some(scs) = te.children().find(|c| c.has_tag_name("speedChanges")) {
            for s in scs.children().filter(|c| c.has_tag_name("speedChange")) {
                res.speed_changes.push(parse_speed_change(&s)?);
            }
        }
        if let Some(lcs) = te.children().find(|c| c.has_tag_name("levelCrossings")) {
            for l in lcs.children().filter(|c| c.has_tag_name("levelCrossing")) {
                res.level_crossings.push(parse_level_crossing(&l)?);
            }
        }
        if let Some(gms) = te.children().find(|c| c.has_tag_name("geoMappings")) {
            for g in gms.children().filter(|c| c.has_tag_name("geoMapping")) {
                res.geo_mappings.push(parse_geo_mapping(&g)?);
            }
        }
    }
    Ok(res)
}

fn parse_cross_section(node: &xml::Node) -> Result<CrossSection, DocErr> {
    Ok(CrossSection {
        id: node.attribute("id").ok_or(DocErr::AttributeMissing("id", node.range().start))?.to_string(),
        name: node.attribute("name").map(|x| x.to_string()),
        ocp_ref: node.attribute("ocpRef").map(|x| x.to_string()),
        pos: parse_position(node)?,
        section_type: node.attribute("type").map(|x| x.to_string()),
    })
}

fn parse_platform_edge(node: &xml::Node) -> Result<PlatformEdge, DocErr> {
    Ok(PlatformEdge {
        id: node
            .attribute("id")
            .ok_or(DocErr::AttributeMissing("id", node.range().start))?
            .to_string(),
        name: node.attribute("name").map(|x| x.to_string()),
        pos: parse_position(node)?,
        dir: parse_direction(node.attribute("dir"), node.range().start)?,
        side: node.attribute("side").map(|x| x.to_string()),
        height: node.attribute("height").and_then(|v| v.parse::<f64>().ok()),
        length: node.attribute("length").and_then(|v| v.parse::<f64>().ok()),
    })
}

fn parse_speed_change(node: &xml::Node) -> Result<SpeedChange, DocErr> {
    Ok(SpeedChange {
        id: node
            .attribute("id")
            .ok_or(DocErr::AttributeMissing("id", node.range().start))?
            .to_string(),
        pos: parse_position(node)?,
        dir: parse_direction(node.attribute("dir"), node.range().start)?,
        vmax: node.attribute("vMax").map(|s| s.to_string()),
        signalised: node
            .attribute("signalised")
            .map(|v| v.parse::<bool>().ok())
            .flatten(),
    })
}

fn parse_level_crossing(node: &xml::Node) -> Result<LevelCrossing, DocErr> {
    Ok(LevelCrossing {
        id: node
            .attribute("id")
            .ok_or(DocErr::AttributeMissing("id", node.range().start))?
            .to_string(),
        pos: parse_position(node)?,
        protection: node.attribute("protection").map(|s| s.to_string()),
        angle: node.attribute("angle").and_then(|v| v.parse::<f64>().ok()),
    })
}

fn parse_geo_mapping(node: &xml::Node) -> Result<GeoMapping, DocErr> {
    Ok(GeoMapping {
        id: node
            .attribute("id")
            .ok_or(DocErr::AttributeMissing("id", node.range().start))?
            .to_string(),
        pos: parse_position(node)?,
        name: node.attribute("name").map(|x| x.to_string()),
        code: node.attribute("code").map(|x| x.to_string()),
        description: node.attribute("description").map(|x| x.to_string()),
    })
}

fn parse_objects(track: &xml::Node) -> Result<Objects, DocErr> {
    let mut signals = Vec::new();
    let mut balises = Vec::new();
    let mut train_detectors = Vec::new();
    let mut track_circuit_borders = Vec::new();
    let mut derailers = Vec::new();
    let mut train_protection_elements = Vec::new();
    let mut train_protection_element_groups = Vec::new();

    if let Some(ocs) = track.children().find(|c| c.has_tag_name("ocsElements")) {
        if let Some(ss) = ocs.children().find(|c| c.has_tag_name("signals")) {
            for s in ss.children().filter(|c| c.has_tag_name("signal")) {
                signals.push(parse_signal(&s)?);
            }
        }
        if let Some(td) = ocs
            .children()
            .find(|c| c.has_tag_name("trainDetectionElements"))
        {
            for det in td.children().filter(|c| c.has_tag_name("trainDetector")) {
                train_detectors.push(parse_train_detector(&det)?);
            }
            for tcb in td
                .children()
                .filter(|c| c.has_tag_name("trackCircuitBorder"))
            {
                track_circuit_borders.push(parse_track_circuit_border(&tcb)?);
            }
        }
        if let Some(bs) = ocs.children().find(|c| c.has_tag_name("balises")) {
            for b in bs.children().filter(|c| c.has_tag_name("balise")) {
                balises.push(parse_balise(&b)?);
            }
        }
        if let Some(der) = ocs.children().find(|c| c.has_tag_name("derailers")) {
            for d in der.children().filter(|c| c.has_tag_name("derailer")) {
                derailers.push(parse_derailer(&d)?);
            }
        }
        if let Some(tp) = ocs
            .children()
            .find(|c| c.has_tag_name("trainProtectionElements"))
        {
            for el in tp
                .children()
                .filter(|c| c.has_tag_name("trainProtectionElement"))
            {
                train_protection_elements.push(parse_train_protection_element(&el)?);
            }
            for grp in tp
                .children()
                .filter(|c| c.has_tag_name("trainProtectionElementGroup"))
            {
                train_protection_element_groups.push(parse_train_protection_group(&grp)?);
            }
        }
    }
    Ok(Objects {
        signals,
        balises,
        train_detectors,
        track_circuit_borders,
        derailers,
        train_protection_elements,
        train_protection_element_groups,
    })
}

fn parse_signal(s: &xml::Node) -> Result<Signal, DocErr> {
    let mut speeds = Vec::new();
    for sp in s.children().filter(|c| c.has_tag_name("speed")) {
        let speed_change_ref = sp
            .children()
            .find(|c| c.has_tag_name("speedChangeRef"))
            .and_then(|c| c.attribute("ref"))
            .map(|x| x.to_string());
        speeds.push(SignalSpeed {
            kind: sp.attribute("kind").map(|x| x.to_string()),
            train_relation: sp.attribute("trainRelation").map(|x| x.to_string()),
            switchable: sp.attribute("switchable").and_then(|v| v.parse::<bool>().ok()),
            speed_change_ref,
        });
    }
    let etcs = s.children().find(|c| c.has_tag_name("etcs")).map(|e| {
        Etcs {
            level_1: e.attribute("level_1").or_else(|| e.attribute("level1")).and_then(|v| v.parse::<bool>().ok()),
            level_2: e.attribute("level_2").or_else(|| e.attribute("level2")).and_then(|v| v.parse::<bool>().ok()),
            level_3: e.attribute("level_3").or_else(|| e.attribute("level3")).and_then(|v| v.parse::<bool>().ok()),
        }
    });
    Ok(Signal {
        id: s
            .attribute("id")
            .ok_or(DocErr::AttributeMissing("id", s.range().start))?
            .to_string(),
        pos: parse_position(s)?,
        name: s.attribute("name").map(|x| x.to_string()),
        dir: parse_direction(s.attribute("dir"), s.range().start)?,
        sight: s.attribute("sight").and_then(|x| x.parse().ok()),
        r#type: match s.attribute("type") {
            Some("distant") => SignalType::Distant,
            Some("repeater") => SignalType::Repeater,
            Some("combined") => SignalType::Combined,
            Some("shunting") => SignalType::Shunting,
            _ => SignalType::Main,
        },
        function: match s.attribute("function") {
            Some("exit") => Some(SignalFunction::Exit),
            Some("home") => Some(SignalFunction::Home),
            Some("blocking") => Some(SignalFunction::Blocking),
            Some("intermediate") => Some(SignalFunction::Intermediate),
            Some(_) => Some(SignalFunction::Other),
            None => None,
        },
        code: s.attribute("code").map(|x| x.to_string()),
        switchable: s.attribute("switchable").and_then(|v| v.parse::<bool>().ok()),
        ocp_station_ref: s.attribute("ocpStationRef").map(|x| x.to_string()),
        speeds,
        etcs,
    })
}

fn parse_train_detector(node: &xml::Node) -> Result<TrainDetector, DocErr> {
    Ok(TrainDetector {
        id: node
            .attribute("id")
            .ok_or(DocErr::AttributeMissing("id", node.range().start))?
            .to_string(),
        pos: parse_position(node)?,
        axle_counting: node.attribute("axleCounting").and_then(|v| v.parse::<bool>().ok()),
        direction_detection: node
            .attribute("directionDetection")
            .and_then(|v| v.parse::<bool>().ok()),
        medium: node.attribute("medium").map(|v| v.to_string()),
    })
}

fn parse_track_circuit_border(node: &xml::Node) -> Result<TrackCircuitBorder, DocErr> {
    Ok(TrackCircuitBorder {
        id: node
            .attribute("id")
            .ok_or(DocErr::AttributeMissing("id", node.range().start))?
            .to_string(),
        pos: parse_position(node)?,
        insulated_rail: node.attribute("insulatedRail").map(|v| v.to_string()),
    })
}

fn parse_derailer(node: &xml::Node) -> Result<Derailer, DocErr> {
    Ok(Derailer {
        id: node
            .attribute("id")
            .ok_or(DocErr::AttributeMissing("id", node.range().start))?
            .to_string(),
        pos: parse_position(node)?,
        dir: node
            .attribute("dir")
            .map(|d| parse_direction(Some(d), node.range().start))
            .transpose()?,
        derail_side: node.attribute("derailSide").map(|v| v.to_string()),
        code: node.attribute("code").map(|v| v.to_string()),
    })
}

fn parse_train_protection_element(node: &xml::Node) -> Result<TrainProtectionElement, DocErr> {
    Ok(TrainProtectionElement {
        id: node
            .attribute("id")
            .ok_or(DocErr::AttributeMissing("id", node.range().start))?
            .to_string(),
        pos: parse_position(node)?,
        dir: node
            .attribute("dir")
            .map(|d| parse_direction(Some(d), node.range().start))
            .transpose()?,
        medium: node.attribute("medium").map(|v| v.to_string()),
        system: node
            .attribute("trainProtectionSystem")
            .map(|v| v.to_string()),
    })
}

fn parse_balise(node: &xml::Node) -> Result<Balise, DocErr> {
    Ok(Balise {
        id: node
            .attribute("id")
            .ok_or(DocErr::AttributeMissing("id", node.range().start))?
            .to_string(),
        pos: parse_position(node)?,
        name: node.attribute("name").map(|x| x.to_string()),
    })
}

fn parse_train_protection_group(node: &xml::Node) -> Result<TrainProtectionElementGroup, DocErr> {
    let mut refs = Vec::new();
    for r in node
        .children()
        .filter(|c| c.has_tag_name("trainProtectionElementRef"))
    {
        if let Some(idr) = r.attribute("ref") {
            refs.push(idr.to_string());
        }
    }
    Ok(TrainProtectionElementGroup {
        id: node
            .attribute("id")
            .ok_or(DocErr::AttributeMissing("id", node.range().start))?
            .to_string(),
        element_refs: refs,
    })
}

fn parse_switches(topo: &xml::Node) -> Result<Vec<Switch>, DocErr> {
    let mut result = Vec::new();
    if let Some(connections) = topo.children().find(|c| c.has_tag_name("connections")) {
        for conn_obj in connections.children().filter(|c| c.is_element()) {
            if conn_obj.has_tag_name("switch") {
                result.push(parse_switch(&conn_obj)?);
            } else if conn_obj.has_tag_name("crossing") {
                result.push(parse_crossing(&conn_obj)?);
            } else {
                return Err(DocErr::UnexpectedElement(
                    format!("{:?}", conn_obj.tag_name()),
                    conn_obj.range().start,
                ));
            }
        }
    }
    Ok(result)
}

fn parse_switch(sw: &xml::Node) -> Result<Switch, DocErr> {
    Ok(Switch::Switch {
        id: sw
            .attribute("id")
            .ok_or(DocErr::AttributeMissing("id", sw.range().start))?
            .to_string(),
        pos: parse_position(sw)?,
        name: sw.attribute("name").map(|x| x.to_string()),
        description: sw.attribute("description").map(|x| x.to_string()),
        length: match sw.attribute("length") {
            Some(length) => Some(
                length
                    .parse::<f64>()
                    .map_err(|_e| DocErr::NumberError(sw.range().start))?,
            ),
            None => None,
        },
        connections: parse_switch_connections(sw)?,
        track_continue_course: match sw.attribute("trackContinueCourse") {
            Some(course) => Some(parse_course(course, sw.range().start)?),
            None => None,
        },
        track_continue_radius: match sw.attribute("trackContinueRadius") {
            Some(rad) => Some(
                rad.parse::<f64>()
                    .map_err(|_e| DocErr::NumberError(sw.range().start))?,
            ),
            None => None,
        },
    })
}

fn parse_switch_connections(sw: &xml::Node) -> Result<Vec<SwitchConnection>, DocErr> {
    let mut result = Vec::new();
    for c in sw
        .children()
        .filter(|x| x.is_element() && x.has_tag_name("connection"))
    {
        result.push(parse_switch_connection(&c)?);
    }
    Ok(result)
}

fn parse_switch_connection(c: &xml::Node) -> Result<SwitchConnection, DocErr> {
    Ok(SwitchConnection {
        id: c
            .attribute("id")
            .ok_or(DocErr::AttributeMissing("id", c.range().start))?
            .to_string(),
        r#ref: c
            .attribute("ref")
            .ok_or(DocErr::AttributeMissing("ref", c.range().start))?
            .to_string(),
        orientation: parse_orientation(
            c.attribute("orientation")
                .ok_or(DocErr::AttributeMissing("orientation", c.range().start))?,
            c.range().start,
        )?,
        course: match c.attribute("course") {
            Some(course) => Some(parse_course(course, c.range().start)?),
            None => None,
        },
        radius: match c.attribute("radius") {
            Some(rad) => Some(
                rad.parse::<f64>()
                    .map_err(|_e| DocErr::NumberError(c.range().start))?,
            ),
            None => None,
        },
        max_speed: match c.attribute("maxSpeed") {
            Some(rad) => Some(
                rad.parse::<f64>()
                    .map_err(|_e| DocErr::NumberError(c.range().start))?,
            ),
            None => None,
        },
        passable: match c.attribute("passable") {
            Some(passable) => Some(
                passable
                    .parse::<bool>()
                    .map_err(|_e| DocErr::BoolError(c.range().start))?,
            ),
            None => None,
        },
    })
}

fn parse_course(x: &str, pos: usize) -> Result<SwitchConnectionCourse, DocErr> {
    match x {
        "left" => Ok(SwitchConnectionCourse::Left),
        "right" => Ok(SwitchConnectionCourse::Right),
        "straight" => Ok(SwitchConnectionCourse::Straight),
        _ => Err(DocErr::EnumErr("left, right, straight", pos)),
    }
}

fn parse_orientation(x: &str, pos: usize) -> Result<ConnectionOrientation, DocErr> {
    match x {
        "incoming" => Ok(ConnectionOrientation::Incoming),
        "outgoing" => Ok(ConnectionOrientation::Outgoing),
        _ => Err(DocErr::EnumErr("incoming, outgoing", pos)),
    }
}

fn parse_crossing(sw: &xml::Node) -> Result<Switch, DocErr> {
    Ok(Switch::Crossing {
        id: sw
            .attribute("id")
            .ok_or(DocErr::AttributeMissing("id", sw.range().start))?
            .to_string(),
        pos: parse_position(sw)?,
        track_continue_course: match sw.attribute("trackContinueCourse") {
            Some(course) => Some(parse_course(course, sw.range().start)?),
            None => None,
        },
        track_continue_radius: match sw.attribute("trackContinueRadius") {
            Some(rad) => Some(
                rad.parse::<f64>()
                    .map_err(|_e| DocErr::NumberError(sw.range().start))?,
            ),
            None => None,
        },
        normal_position: match sw.attribute("normalPosition") {
            Some(course) => Some(parse_course(course, sw.range().start)?),
            None => None,
        },
        length: match sw.attribute("length") {
            Some(length) => Some(
                length
                    .parse::<f64>()
                    .map_err(|_e| DocErr::NumberError(sw.range().start))?,
            ),
            None => None,
        },
        connections: parse_switch_connections(sw)?,
    })
}

fn parse_track_node(node: &xml::Node) -> Result<Node, DocErr> {
    Ok(Node {
        id: node
            .attribute("id")
            .ok_or(DocErr::AttributeMissing("id", node.range().start))?
            .to_string(),
        pos: parse_position(node)?,
        connection: parse_track_connection(node)?,
    })
}

fn parse_track_connection(node: &xml::Node) -> Result<TrackEndConnection, DocErr> {
    if let Some(e) = node.children().find(|c| c.has_tag_name("connection")) {
        let id = e
            .attribute("id")
            .ok_or(DocErr::AttributeMissing("id", e.range().start))?;
        let idref = e
            .attribute("ref")
            .ok_or(DocErr::AttributeMissing("ref", e.range().start))?;
        return Ok(TrackEndConnection::Connection(id.to_string(), idref.to_string()));
    }
    if let Some(_e) = node.children().find(|c| c.has_tag_name("bufferStop")) {
        return Ok(TrackEndConnection::BufferStop);
    }
    if let Some(_e) = node.children().find(|c| c.has_tag_name("openEnd")) {
        return Ok(TrackEndConnection::OpenEnd);
    }
    if let Some(e) = node.children().find(|c| c.has_tag_name("macroscopicNode")) {
        let id = e
            .attribute("id")
            .ok_or(DocErr::AttributeMissing("id", e.range().start))?;
        return Ok(TrackEndConnection::MacroscopicNode(id.to_string()));
    }
    Err(DocErr::ElementMissing(
        "connection or bufferStop or openEnd or macroscopicNode",
        node.range().start,
    ))
}

fn parse_position(node: &xml::Node) -> Result<Position, DocErr> {
    Ok(Position {
        offset: node
            .attribute("pos")
            .ok_or(DocErr::AttributeMissing("pos", node.range().start))?
            .parse::<f64>()
            .map_err(|_e| DocErr::NumberError(node.range().start))?,
        mileage: match node.attribute("absPos") {
            Some(abs_pos) => Some(
                abs_pos
                    .parse::<f64>()
                    .map_err(|_e| DocErr::NumberError(node.range().start))?,
            ),
            None => None,
        },
        geo_coord: node
            .children()
            .find(|c| c.has_tag_name("geoCoord"))
            .and_then(|c| c.attribute("coord"))
            .map(|x| x.to_string())
            .or_else(|| node.attribute("geoCoord").map(|x| x.to_string())),
    })
}

fn parse_direction(dir: Option<&str>, pos: usize) -> Result<TrackDirection, DocErr> {
    match dir {
        Some("up") => Ok(TrackDirection::Up),
        Some("down") | None => Ok(TrackDirection::Down),
        Some(_) => Err(DocErr::EnumErr("up, down", pos)),
    }
}
