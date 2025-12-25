use crate::model::*;

fn escape_attr(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}

fn push_indent(out: &mut String, level: usize) {
    for _ in 0..level {
        out.push_str("  ");
    }
}

fn push_attr(out: &mut String, key: &str, value: &str) {
    out.push(' ');
    out.push_str(key);
    out.push_str("=\"");
    out.push_str(&escape_attr(value));
    out.push('"');
}

fn fmt_f64(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{:.1}", v)
    } else {
        format!("{}", v)
    }
}

fn write_position_attrs(out: &mut String, pos: &Position) {
    push_attr(out, "pos", &fmt_f64(pos.offset));
    if let Some(abs) = pos.mileage {
        push_attr(out, "absPos", &fmt_f64(abs));
    }
}

fn write_track_direction(out: &mut String, dir: TrackDirection) {
    let dir_str = match dir {
        TrackDirection::Up => "up",
        TrackDirection::Down => "down",
    };
    push_attr(out, "dir", dir_str);
}

fn write_signal_type(out: &mut String, t: SignalType) {
    let s = match t {
        SignalType::Main => "main",
        SignalType::Distant => "distant",
        SignalType::Repeater => "repeater",
        SignalType::Combined => "combined",
        SignalType::Shunting => "shunting",
    };
    push_attr(out, "type", s);
}

fn write_signal_function(out: &mut String, f: SignalFunction) {
    let s = match f {
        SignalFunction::Exit => "exit",
        SignalFunction::Home => "home",
        SignalFunction::Blocking => "blocking",
        SignalFunction::Intermediate => "intermediate",
        SignalFunction::Other => "other",
    };
    push_attr(out, "function", s);
}

fn write_orientation(out: &mut String, orientation: &ConnectionOrientation) {
    let s = match orientation {
        ConnectionOrientation::Incoming => "incoming",
        ConnectionOrientation::Outgoing => "outgoing",
        ConnectionOrientation::RightAngled => "rightAngled",
        ConnectionOrientation::Unknown => "unknown",
        ConnectionOrientation::Other => "other",
    };
    push_attr(out, "orientation", s);
}

fn write_course(out: &mut String, course: SwitchConnectionCourse) {
    let s = match course {
        SwitchConnectionCourse::Straight => "straight",
        SwitchConnectionCourse::Left => "left",
        SwitchConnectionCourse::Right => "right",
    };
    push_attr(out, "course", s);
}

fn write_track_end_connection(out: &mut String, conn: &TrackEndConnection, level: usize) {
    match conn {
        TrackEndConnection::Connection(id, idref) => {
            push_indent(out, level);
            out.push_str("<connection");
            push_attr(out, "id", id);
            push_attr(out, "ref", idref);
            out.push_str("/>\n");
        }
        TrackEndConnection::BufferStop => {
            push_indent(out, level);
            out.push_str("<bufferStop/>\n");
        }
        TrackEndConnection::OpenEnd => {
            push_indent(out, level);
            out.push_str("<openEnd/>\n");
        }
        TrackEndConnection::MacroscopicNode(id) => {
            push_indent(out, level);
            out.push_str("<macroscopicNode");
            push_attr(out, "id", id);
            out.push_str("/>\n");
        }
    }
}

fn write_switch(out: &mut String, sw: &Switch, level: usize) {
    match sw {
        Switch::Switch {
            id,
            pos,
            name,
            description,
            length,
            connections,
            track_continue_course,
            track_continue_radius,
        } => {
            push_indent(out, level);
            out.push_str("<switch");
            push_attr(out, "id", id);
            write_position_attrs(out, pos);
            if let Some(name) = name {
                push_attr(out, "name", name);
            }
            if let Some(desc) = description {
                push_attr(out, "description", desc);
            }
            if let Some(len) = length {
                push_attr(out, "length", &fmt_f64(*len));
            }
            if let Some(course) = track_continue_course {
                write_course(out, *course);
            }
            if let Some(radius) = track_continue_radius {
                push_attr(out, "trackContinueRadius", &fmt_f64(*radius));
            }
            out.push_str(">\n");
            for conn in connections {
                push_indent(out, level + 1);
                out.push_str("<connection");
                push_attr(out, "id", &conn.id);
                push_attr(out, "ref", &conn.r#ref);
                write_orientation(out, &conn.orientation);
                if let Some(course) = conn.course {
                    write_course(out, course);
                }
                if let Some(radius) = conn.radius {
                    push_attr(out, "radius", &fmt_f64(radius));
                }
                if let Some(max_speed) = conn.max_speed {
                    push_attr(out, "maxSpeed", &fmt_f64(max_speed));
                }
                if let Some(passable) = conn.passable {
                    push_attr(out, "passable", if passable { "true" } else { "false" });
                }
                out.push_str("/>\n");
            }
            push_indent(out, level);
            out.push_str("</switch>\n");
        }
        Switch::Crossing {
            id,
            pos,
            track_continue_course,
            track_continue_radius,
            normal_position,
            length,
            connections,
        } => {
            push_indent(out, level);
            out.push_str("<crossing");
            push_attr(out, "id", id);
            write_position_attrs(out, pos);
            if let Some(course) = track_continue_course {
                write_course(out, *course);
            }
            if let Some(radius) = track_continue_radius {
                push_attr(out, "trackContinueRadius", &fmt_f64(*radius));
            }
            if let Some(course) = normal_position {
                write_course(out, *course);
            }
            if let Some(len) = length {
                push_attr(out, "length", &fmt_f64(*len));
            }
            out.push_str(">\n");
            for conn in connections {
                push_indent(out, level + 1);
                out.push_str("<connection");
                push_attr(out, "id", &conn.id);
                push_attr(out, "ref", &conn.r#ref);
                write_orientation(out, &conn.orientation);
                if let Some(course) = conn.course {
                    write_course(out, course);
                }
                out.push_str("/>\n");
            }
            push_indent(out, level);
            out.push_str("</crossing>\n");
        }
    }
}

fn write_track_elements(out: &mut String, track: &Track, level: usize) {
    if track.track_elements.platform_edges.is_empty()
        && track.track_elements.speed_changes.is_empty()
        && track.track_elements.level_crossings.is_empty()
    {
        return;
    }

    push_indent(out, level);
    out.push_str("<trackElements>\n");

    if !track.track_elements.platform_edges.is_empty() {
        push_indent(out, level + 1);
        out.push_str("<platformEdges>\n");
        for pe in &track.track_elements.platform_edges {
            push_indent(out, level + 2);
            out.push_str("<platformEdge");
            push_attr(out, "id", &pe.id);
            write_position_attrs(out, &pe.pos);
            write_track_direction(out, pe.dir);
            if let Some(name) = &pe.name {
                push_attr(out, "name", name);
            }
            if let Some(side) = &pe.side {
                push_attr(out, "side", side);
            }
            if let Some(height) = pe.height {
                push_attr(out, "height", &fmt_f64(height));
            }
            if let Some(length) = pe.length {
                push_attr(out, "length", &fmt_f64(length));
            }
            out.push_str("/>\n");
        }
        push_indent(out, level + 1);
        out.push_str("</platformEdges>\n");
    }

    if !track.track_elements.speed_changes.is_empty() {
        push_indent(out, level + 1);
        out.push_str("<speedChanges>\n");
        for sc in &track.track_elements.speed_changes {
            push_indent(out, level + 2);
            out.push_str("<speedChange");
            push_attr(out, "id", &sc.id);
            write_position_attrs(out, &sc.pos);
            write_track_direction(out, sc.dir);
            if let Some(vmax) = &sc.vmax {
                push_attr(out, "vMax", vmax);
            }
            if let Some(signalised) = sc.signalised {
                push_attr(out, "signalised", if signalised { "true" } else { "false" });
            }
            out.push_str("/>\n");
        }
        push_indent(out, level + 1);
        out.push_str("</speedChanges>\n");
    }

    if !track.track_elements.level_crossings.is_empty() {
        push_indent(out, level + 1);
        out.push_str("<levelCrossings>\n");
        for lc in &track.track_elements.level_crossings {
            push_indent(out, level + 2);
            out.push_str("<levelCrossing");
            push_attr(out, "id", &lc.id);
            write_position_attrs(out, &lc.pos);
            if let Some(protection) = &lc.protection {
                push_attr(out, "protection", protection);
            }
            if let Some(angle) = lc.angle {
                push_attr(out, "angle", &fmt_f64(angle));
            }
            out.push_str("/>\n");
        }
        push_indent(out, level + 1);
        out.push_str("</levelCrossings>\n");
    }

    push_indent(out, level);
    out.push_str("</trackElements>\n");
}

fn write_cross_sections(out: &mut String, track: &Track, level: usize) {
    if track.track_elements.cross_sections.is_empty() {
        return;
    }
    push_indent(out, level);
    out.push_str("<crossSections>\n");
    for cs in &track.track_elements.cross_sections {
        push_indent(out, level + 1);
        out.push_str("<crossSection");
        push_attr(out, "id", &cs.id);
        write_position_attrs(out, &cs.pos);
        if let Some(name) = &cs.name {
            push_attr(out, "name", name);
        }
        if let Some(ocp) = &cs.ocp_ref {
            push_attr(out, "ocpRef", ocp);
        }
        if let Some(section_type) = &cs.section_type {
            push_attr(out, "type", section_type);
        }
        out.push_str("/>\n");
    }
    push_indent(out, level);
    out.push_str("</crossSections>\n");
}

fn write_objects(out: &mut String, objs: &Objects, level: usize) {
    if objs.signals.is_empty()
        && objs.balises.is_empty()
        && objs.train_detectors.is_empty()
        && objs.track_circuit_borders.is_empty()
        && objs.derailers.is_empty()
        && objs.train_protection_elements.is_empty()
        && objs.train_protection_element_groups.is_empty()
    {
        return;
    }

    push_indent(out, level);
    out.push_str("<ocsElements>\n");

    if !objs.signals.is_empty() {
        push_indent(out, level + 1);
        out.push_str("<signals>\n");
        for sig in &objs.signals {
            push_indent(out, level + 2);
            out.push_str("<signal");
            push_attr(out, "id", &sig.id);
            write_position_attrs(out, &sig.pos);
            if let Some(name) = &sig.name {
                push_attr(out, "name", name);
            }
            write_track_direction(out, sig.dir);
            write_signal_type(out, sig.r#type);
            if let Some(func) = sig.function {
                write_signal_function(out, func);
            }
            if let Some(code) = &sig.code {
                push_attr(out, "code", code);
            }
            if let Some(sw) = sig.switchable {
                push_attr(out, "switchable", if sw { "true" } else { "false" });
            }
            if let Some(ocp) = &sig.ocp_station_ref {
                push_attr(out, "ocpStationRef", ocp);
            }
            out.push_str("/>\n");
        }
        push_indent(out, level + 1);
        out.push_str("</signals>\n");
    }

    if !objs.train_detectors.is_empty() || !objs.track_circuit_borders.is_empty() {
        push_indent(out, level + 1);
        out.push_str("<trainDetectionElements>\n");
        for det in &objs.train_detectors {
            push_indent(out, level + 2);
            out.push_str("<trainDetector");
            push_attr(out, "id", &det.id);
            write_position_attrs(out, &det.pos);
            if let Some(axle) = det.axle_counting {
                push_attr(out, "axleCounting", if axle { "true" } else { "false" });
            }
            if let Some(direction) = det.direction_detection {
                push_attr(out, "directionDetection", if direction { "true" } else { "false" });
            }
            if let Some(medium) = &det.medium {
                push_attr(out, "medium", medium);
            }
            out.push_str("/>\n");
        }
        for tcb in &objs.track_circuit_borders {
            push_indent(out, level + 2);
            out.push_str("<trackCircuitBorder");
            push_attr(out, "id", &tcb.id);
            write_position_attrs(out, &tcb.pos);
            if let Some(rail) = &tcb.insulated_rail {
                push_attr(out, "insulatedRail", rail);
            }
            out.push_str("/>\n");
        }
        push_indent(out, level + 1);
        out.push_str("</trainDetectionElements>\n");
    }

    if !objs.balises.is_empty() {
        push_indent(out, level + 1);
        out.push_str("<balises>\n");
        for b in &objs.balises {
            push_indent(out, level + 2);
            out.push_str("<balise");
            push_attr(out, "id", &b.id);
            write_position_attrs(out, &b.pos);
            if let Some(name) = &b.name {
                push_attr(out, "name", name);
            }
            out.push_str("/>\n");
        }
        push_indent(out, level + 1);
        out.push_str("</balises>\n");
    }

    if !objs.derailers.is_empty() {
        push_indent(out, level + 1);
        out.push_str("<derailers>\n");
        for d in &objs.derailers {
            push_indent(out, level + 2);
            out.push_str("<derailer");
            push_attr(out, "id", &d.id);
            write_position_attrs(out, &d.pos);
            if let Some(dir) = d.dir {
                write_track_direction(out, dir);
            }
            if let Some(side) = &d.derail_side {
                push_attr(out, "derailSide", side);
            }
            if let Some(code) = &d.code {
                push_attr(out, "code", code);
            }
            out.push_str("/>\n");
        }
        push_indent(out, level + 1);
        out.push_str("</derailers>\n");
    }

    if !objs.train_protection_elements.is_empty() || !objs.train_protection_element_groups.is_empty() {
        push_indent(out, level + 1);
        out.push_str("<trainProtectionElements>\n");
        for tpe in &objs.train_protection_elements {
            push_indent(out, level + 2);
            out.push_str("<trainProtectionElement");
            push_attr(out, "id", &tpe.id);
            write_position_attrs(out, &tpe.pos);
            if let Some(dir) = tpe.dir {
                write_track_direction(out, dir);
            }
            if let Some(medium) = &tpe.medium {
                push_attr(out, "medium", medium);
            }
            if let Some(system) = &tpe.system {
                push_attr(out, "trainProtectionSystem", system);
            }
            out.push_str("/>\n");
        }
        for group in &objs.train_protection_element_groups {
            push_indent(out, level + 2);
            out.push_str("<trainProtectionElementGroup");
            push_attr(out, "id", &group.id);
            if group.element_refs.is_empty() {
                out.push_str("/>\n");
            } else {
                out.push_str(">\n");
                for r in &group.element_refs {
                    push_indent(out, level + 3);
                    out.push_str("<trainProtectionElementRef");
                    push_attr(out, "ref", r);
                    out.push_str("/>\n");
                }
                push_indent(out, level + 2);
                out.push_str("</trainProtectionElementGroup>\n");
            }
        }
        push_indent(out, level + 1);
        out.push_str("</trainProtectionElements>\n");
    }

    push_indent(out, level);
    out.push_str("</ocsElements>\n");
}

pub fn write_railml(railml: &RailML) -> String {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n");
    out.push_str("<railml xmlns=\"https://www.railml.org/schemas/2021\" ");
    out.push_str("xmlns:dc=\"http://purl.org/dc/elements/1.1/\" ");
    out.push_str("xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" ");
    out.push_str("xsi:schemaLocation=\"https://www.railml.org/schemas/2021 https://schemas.railml.org/2021/railML-2.5/schema/railML.xsd\" ");
    out.push_str("version=\"2.5\">\n");

    if let Some(infra) = &railml.infrastructure {
        push_indent(&mut out, 1);
        out.push_str("<infrastructure id=\"inf01\">\n");
        push_indent(&mut out, 2);
        out.push_str("<tracks>\n");
        for track in &infra.tracks {
            push_indent(&mut out, 3);
            out.push_str("<track");
            push_attr(&mut out, "id", &track.id);
            if let Some(name) = &track.name {
                push_attr(&mut out, "name", name);
            }
            if let Some(code) = &track.code {
                push_attr(&mut out, "code", code);
            }
            if let Some(desc) = &track.description {
                push_attr(&mut out, "description", desc);
            }
            if let Some(tt) = &track.track_type {
                push_attr(&mut out, "type", tt);
            }
            if let Some(dir) = &track.main_dir {
                push_attr(&mut out, "mainDir", dir);
            }
            out.push_str(">\n");

            push_indent(&mut out, 4);
            out.push_str("<trackTopology>\n");

            push_indent(&mut out, 5);
            out.push_str("<trackBegin");
            push_attr(&mut out, "id", &track.begin.id);
            write_position_attrs(&mut out, &track.begin.pos);
            out.push_str(">\n");
            write_track_end_connection(&mut out, &track.begin.connection, 6);
            push_indent(&mut out, 5);
            out.push_str("</trackBegin>\n");

            push_indent(&mut out, 5);
            out.push_str("<trackEnd");
            push_attr(&mut out, "id", &track.end.id);
            write_position_attrs(&mut out, &track.end.pos);
            out.push_str(">\n");
            write_track_end_connection(&mut out, &track.end.connection, 6);
            push_indent(&mut out, 5);
            out.push_str("</trackEnd>\n");

            if !track.switches.is_empty() {
                push_indent(&mut out, 5);
                out.push_str("<connections>\n");
                for sw in &track.switches {
                    write_switch(&mut out, sw, 6);
                }
                push_indent(&mut out, 5);
                out.push_str("</connections>\n");
            }

            write_cross_sections(&mut out, track, 5);

            push_indent(&mut out, 4);
            out.push_str("</trackTopology>\n");

            write_track_elements(&mut out, track, 4);
            write_objects(&mut out, &track.objects, 4);

            push_indent(&mut out, 3);
            out.push_str("</track>\n");
        }
        push_indent(&mut out, 2);
        out.push_str("</tracks>\n");
        push_indent(&mut out, 1);
        out.push_str("</infrastructure>\n");
    }

    out.push_str("</railml>\n");
    out
}
