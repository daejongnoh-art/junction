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

fn write_geo_coord(out: &mut String, coord: &str, level: usize) {
    push_indent(out, level);
    out.push_str("<geoCoord");
    push_attr(out, "coord", coord);
    out.push_str("/>\n");
}

fn write_text_element(out: &mut String, tag: &str, value: &str, level: usize) {
    push_indent(out, level);
    out.push('<');
    out.push_str(tag);
    out.push('>');
    out.push_str(&escape_attr(value));
    out.push_str("</");
    out.push_str(tag);
    out.push_str(">\n");
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
            if let Some(gc) = &pos.geo_coord {
                write_geo_coord(out, gc, level + 1);
            }
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
            if let Some(gc) = &pos.geo_coord {
                write_geo_coord(out, gc, level + 1);
            }
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
        && track.track_elements.geo_mappings.is_empty()
    {
        return;
    }

    push_indent(out, level);
    out.push_str("<trackElements>\n");

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

    if !track.track_elements.geo_mappings.is_empty() {
        push_indent(out, level + 1);
        out.push_str("<geoMappings>\n");
        for gm in &track.track_elements.geo_mappings {
            push_indent(out, level + 2);
            out.push_str("<geoMapping");
            push_attr(out, "id", &gm.id);
            write_position_attrs(out, &gm.pos);
            if let Some(name) = &gm.name {
                push_attr(out, "name", name);
            }
            if let Some(code) = &gm.code {
                push_attr(out, "code", code);
            }
            if let Some(desc) = &gm.description {
                push_attr(out, "description", desc);
            }
            if let Some(gc) = &gm.pos.geo_coord {
                out.push_str(">\n");
                write_geo_coord(out, gc, level + 3);
                push_indent(out, level + 2);
                out.push_str("</geoMapping>\n");
            } else {
                out.push_str("/>\n");
            }
        }
        push_indent(out, level + 1);
        out.push_str("</geoMappings>\n");
    }

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
            if sig.etcs.is_none() && sig.speeds.is_empty() {
                out.push_str("/>\n");
            } else {
                out.push_str(">\n");
                if let Some(etcs) = &sig.etcs {
                    push_indent(out, level + 3);
                    out.push_str("<etcs");
                    if let Some(v) = etcs.level_1 {
                        push_attr(out, "level_1", if v { "true" } else { "false" });
                    }
                    if let Some(v) = etcs.level_2 {
                        push_attr(out, "level_2", if v { "true" } else { "false" });
                    }
                    if let Some(v) = etcs.level_3 {
                        push_attr(out, "level_3", if v { "true" } else { "false" });
                    }
                    out.push_str("/>\n");
                }
                for sp in &sig.speeds {
                    push_indent(out, level + 3);
                    out.push_str("<speed");
                    if let Some(kind) = &sp.kind {
                        push_attr(out, "kind", kind);
                    }
                    if let Some(rel) = &sp.train_relation {
                        push_attr(out, "trainRelation", rel);
                    }
                    if let Some(sw) = sp.switchable {
                        push_attr(out, "switchable", if sw { "true" } else { "false" });
                    }
                    if let Some(r) = &sp.speed_change_ref {
                        out.push_str(">\n");
                        push_indent(out, level + 4);
                        out.push_str("<speedChangeRef");
                        push_attr(out, "ref", r);
                        out.push_str("/>\n");
                        push_indent(out, level + 3);
                        out.push_str("</speed>\n");
                    } else {
                        out.push_str("/>\n");
                    }
                }
                push_indent(out, level + 2);
                out.push_str("</signal>\n");
            }
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

fn write_metadata(out: &mut String, md: &Metadata, level: usize) {
    push_indent(out, level);
    out.push_str("<metadata");
    if let Some(v) = &md.version {
        push_attr(out, "version", v);
    }
    out.push_str(">\n");

    if let Some(v) = &md.dc_format {
        write_text_element(out, "format", v, level + 1);
    }
    if let Some(v) = &md.dc_identifier {
        write_text_element(out, "identifier", v, level + 1);
    }
    if let Some(v) = &md.dc_source {
        write_text_element(out, "source", v, level + 1);
    }
    if let Some(v) = &md.dc_title {
        write_text_element(out, "title", v, level + 1);
    }
    if let Some(v) = &md.dc_language {
        write_text_element(out, "language", v, level + 1);
    }
    if let Some(v) = &md.dc_creator {
        write_text_element(out, "creator", v, level + 1);
    }
    if let Some(v) = &md.dc_description {
        write_text_element(out, "description", v, level + 1);
    }
    if let Some(v) = &md.dc_rights {
        write_text_element(out, "rights", v, level + 1);
    }

    if !md.organizational_units.is_empty() {
        push_indent(out, level + 1);
        out.push_str("<organizationalUnits>\n");
        for ou in &md.organizational_units {
            push_indent(out, level + 2);
            out.push_str("<infrastructureManager");
            push_attr(out, "id", &ou.id);
            if let Some(code) = &ou.code {
                push_attr(out, "code", code);
            }
            if let Some(name) = &ou.name {
                push_attr(out, "name", name);
            }
            if let Some(contact) = &ou.contact {
                push_attr(out, "contact", contact);
            }
            out.push_str("/>\n");
        }
        push_indent(out, level + 1);
        out.push_str("</organizationalUnits>\n");
    }

    push_indent(out, level);
    out.push_str("</metadata>\n");
}

fn write_track_groups(out: &mut String, infra: &Infrastructure, level: usize) {
    if infra.track_groups.is_empty() {
        return;
    }
    push_indent(out, level);
    out.push_str("<trackGroups>\n");
    for line in &infra.track_groups {
        push_indent(out, level + 1);
        out.push_str("<line");
        push_attr(out, "id", &line.id);
        if let Some(code) = &line.code {
            push_attr(out, "code", code);
        }
        if let Some(name) = &line.name {
            push_attr(out, "name", name);
        }
        if let Some(im) = &line.infrastructure_manager_ref {
            push_attr(out, "infrastructureManagerRef", im);
        }
        if let Some(cat) = &line.line_category {
            push_attr(out, "lineCategory", cat);
        }
        if let Some(ty) = &line.line_type {
            push_attr(out, "type", ty);
        }
        if line.track_refs.is_empty() && line.additional_names.is_empty() {
            out.push_str("/>\n");
            continue;
        }
        out.push_str(">\n");
        for an in &line.additional_names {
            push_indent(out, level + 2);
            out.push_str("<additionalName");
            push_attr(out, "name", &an.name);
            if let Some(lang) = &an.lang {
                push_attr(out, "xml:lang", lang);
            }
            if let Some(t) = &an.name_type {
                push_attr(out, "type", t);
            }
            out.push_str("/>\n");
        }
        for tr in &line.track_refs {
            push_indent(out, level + 2);
            out.push_str("<trackRef");
            push_attr(out, "ref", &tr.r#ref);
            if let Some(seq) = tr.sequence {
                push_attr(out, "sequence", &seq.to_string());
            }
            out.push_str("/>\n");
        }
        push_indent(out, level + 1);
        out.push_str("</line>\n");
    }
    push_indent(out, level);
    out.push_str("</trackGroups>\n");
}

fn write_operation_control_points(out: &mut String, infra: &Infrastructure, level: usize) {
    if infra.ocps.is_empty() {
        return;
    }
    push_indent(out, level);
    out.push_str("<operationControlPoints>\n");
    for ocp in &infra.ocps {
        push_indent(out, level + 1);
        out.push_str("<ocp");
        push_attr(out, "id", &ocp.id);
        if let Some(name) = &ocp.name {
            push_attr(out, "name", name);
        }
        if let Some(lang) = &ocp.lang {
            push_attr(out, "xml:lang", lang);
        }
        if let Some(t) = &ocp.r#type {
            push_attr(out, "type", t);
        }
        if ocp.additional_names.is_empty()
            && ocp.prop_operational.is_none()
            && ocp.prop_equipment.is_none()
            && ocp.prop_service.is_none()
            && ocp.designator.is_none()
            && ocp.geo_coord.is_none()
        {
            out.push_str("/>\n");
            continue;
        }
        out.push_str(">\n");

        for an in &ocp.additional_names {
            push_indent(out, level + 2);
            out.push_str("<additionalName");
            push_attr(out, "name", &an.name);
            if let Some(lang) = &an.lang {
                push_attr(out, "xml:lang", lang);
            }
            if let Some(t) = &an.name_type {
                push_attr(out, "type", t);
            }
            out.push_str("/>\n");
        }

        if let Some(prop) = &ocp.prop_operational {
            push_indent(out, level + 2);
            out.push_str("<propOperational");
            if let Some(v) = prop.ensures_train_sequence {
                push_attr(out, "ensuresTrainSequence", if v { "true" } else { "false" });
            }
            if let Some(v) = prop.order_changeable {
                push_attr(out, "orderChangeable", if v { "true" } else { "false" });
            }
            if let Some(v) = &prop.operational_type {
                push_attr(out, "operationalType", v);
            }
            if let Some(v) = &prop.traffic_type {
                push_attr(out, "trafficType", v);
            }
            out.push_str("/>\n");
        }

        if let Some(prop) = &ocp.prop_service {
            push_indent(out, level + 2);
            out.push_str("<propService");
            if let Some(v) = prop.passenger {
                push_attr(out, "passenger", if v { "true" } else { "false" });
            }
            if let Some(v) = prop.service {
                push_attr(out, "service", if v { "true" } else { "false" });
            }
            if let Some(v) = prop.goods_siding {
                push_attr(out, "goodsSiding", if v { "true" } else { "false" });
            }
            out.push_str("/>\n");
        }

        if let Some(prop) = &ocp.prop_equipment {
            push_indent(out, level + 2);
            out.push_str("<propEquipment");
            if prop.summary.is_none() && prop.track_refs.is_empty() {
                out.push_str("/>\n");
            } else {
                out.push_str(">\n");
                if let Some(summary) = &prop.summary {
                    push_indent(out, level + 3);
                    out.push_str("<summary");
                    if let Some(v) = summary.has_home_signals {
                        push_attr(out, "hasHomeSignals", if v { "true" } else { "false" });
                    }
                    if let Some(v) = summary.has_starter_signals {
                        push_attr(out, "hasStarterSignals", if v { "true" } else { "false" });
                    }
                    if let Some(v) = summary.has_switches {
                        push_attr(out, "hasSwitches", if v { "true" } else { "false" });
                    }
                    if let Some(v) = &summary.signal_box {
                        push_attr(out, "signalBox", v);
                    }
                    out.push_str("/>\n");
                }
                for tr in &prop.track_refs {
                    push_indent(out, level + 3);
                    out.push_str("<trackRef");
                    push_attr(out, "ref", tr);
                    out.push_str("/>\n");
                }
                push_indent(out, level + 2);
                out.push_str("</propEquipment>\n");
            }
        }

        if let Some(gc) = &ocp.geo_coord {
            push_indent(out, level + 2);
            out.push_str("<geoCoord");
            push_attr(out, "coord", &gc.coord);
            if let Some(code) = &gc.epsg_code {
                push_attr(out, "epsgCode", code);
            }
            out.push_str("/>\n");
        }

        if let Some(des) = &ocp.designator {
            push_indent(out, level + 2);
            out.push_str("<designator");
            if let Some(reg) = &des.register {
                push_attr(out, "register", reg);
            }
            if let Some(entry) = &des.entry {
                push_attr(out, "entry", entry);
            }
            out.push_str("/>\n");
        }

        push_indent(out, level + 1);
        out.push_str("</ocp>\n");
    }
    push_indent(out, level);
    out.push_str("</operationControlPoints>\n");
}

fn write_states(out: &mut String, infra: &Infrastructure, level: usize) {
    if infra.states.is_empty() {
        return;
    }
    push_indent(out, level);
    out.push_str("<states>\n");
    for state in &infra.states {
        push_indent(out, level + 1);
        out.push_str("<state");
        push_attr(out, "id", &state.id);
        if let Some(disabled) = state.disabled {
            push_attr(out, "disabled", if disabled { "true" } else { "false" });
        }
        if let Some(status) = &state.status {
            push_attr(out, "status", status);
        }
        out.push_str("/>\n");
    }
    push_indent(out, level);
    out.push_str("</states>\n");
}

fn write_rollingstock(out: &mut String, rs: &Rollingstock, level: usize) {
    if rs.vehicles.is_empty() {
        return;
    }

    push_indent(out, level);
    out.push_str("<rollingstock>\n");
    push_indent(out, level + 1);
    out.push_str("<vehicles>\n");
    for vehicle in &rs.vehicles {
        push_indent(out, level + 2);
        out.push_str("<vehicle");
        push_attr(out, "id", &vehicle.id);
        if let Some(name) = &vehicle.name {
            push_attr(out, "name", name);
        }
        if let Some(desc) = &vehicle.description {
            push_attr(out, "description", desc);
        }
        if let Some(length) = vehicle.length {
            push_attr(out, "length", &format!("{}", length));
        }
        if let Some(speed) = vehicle.speed {
            push_attr(out, "speed", &format!("{}", speed));
        }
        out.push_str("/>\n");
    }
    push_indent(out, level + 1);
    out.push_str("</vehicles>\n");
    push_indent(out, level);
    out.push_str("</rollingstock>\n");
}

pub fn write_railml(railml: &RailML) -> String {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n");
    out.push_str("<railml xmlns=\"https://www.railml.org/schemas/2021\" ");
    out.push_str("xmlns:dc=\"http://purl.org/dc/elements/1.1/\" ");
    out.push_str("xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" ");
    out.push_str("xsi:schemaLocation=\"https://www.railml.org/schemas/2021 https://schemas.railml.org/2021/railML-2.5/schema/railML.xsd\" ");
    out.push_str("version=\"2.5\">\n");

    if let Some(md) = &railml.metadata {
        write_metadata(&mut out, md, 1);
    }

    if let Some(infra) = &railml.infrastructure {
        push_indent(&mut out, 1);
        out.push_str("<infrastructure id=\"inf01\">\n");
        write_operation_control_points(&mut out, infra, 2);
        write_track_groups(&mut out, infra, 2);
        write_states(&mut out, infra, 2);
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
            if let Some(gc) = &track.begin.pos.geo_coord {
                write_geo_coord(&mut out, gc, 6);
            }
            write_track_end_connection(&mut out, &track.begin.connection, 6);
            push_indent(&mut out, 5);
            out.push_str("</trackBegin>\n");

            push_indent(&mut out, 5);
            out.push_str("<trackEnd");
            push_attr(&mut out, "id", &track.end.id);
            write_position_attrs(&mut out, &track.end.pos);
            out.push_str(">\n");
            if let Some(gc) = &track.end.pos.geo_coord {
                write_geo_coord(&mut out, gc, 6);
            }
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

    if let Some(rs) = &railml.rollingstock {
        write_rollingstock(&mut out, rs, 1);
    }

    out.push_str("</railml>\n");
    out
}
