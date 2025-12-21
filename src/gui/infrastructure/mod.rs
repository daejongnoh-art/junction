pub mod draw;
pub mod menus;

use const_cstr::*;
use matches::matches;
use backend_glfw::imgui::*;
use nalgebra_glm as glm;

use crate::util;
use crate::app::App;
use crate::config::*;
use crate::document::*;
use crate::document::infview::*;
use crate::document::view::*;
use crate::document::interlocking::*;
use crate::document::model::*;
use crate::document::analysis::*;
use crate::document::dispatch::*;
use crate::document::objects::*;
use crate::gui::widgets;
use crate::gui::widgets::Draw;
use crate::config::RailUIColorName;


#[derive(Copy,Clone,Debug)]
pub enum Highlight {
    Ref(Ref),
    Tvd(usize),
}

pub fn inf_view(config :&Config, 
                analysis :&mut Analysis,
                inf_view :&mut InfView,
                dispatch_view :&mut Option<DispatchView>) -> Draw {
    unsafe {
        let pos_before : ImVec2 = igGetCursorPos_nonUDT2().into();

        let size = igGetContentRegionAvail_nonUDT2().into();
        let draw = widgets::canvas(size,
                        config.color_u32(RailUIColorName::CanvasBackground),
                        const_cstr!("railwaycanvas").as_ptr());
        draw.begin_draw();
        scroll(inf_view);
        let mut preview_route = None;
        context_menu(analysis, inf_view, dispatch_view, &draw, &mut preview_route);
        interact(config, analysis, inf_view, &draw);
        draw_inf(config, analysis, inf_view, dispatch_view, &draw, preview_route);
        draw.end_draw();

        let pos_after = igGetCursorPos_nonUDT2().into();
        let framespace = igGetFrameHeightWithSpacing() - igGetFrameHeight();
        igSetCursorPos(pos_before + ImVec2 { x: 2.0*framespace, y: 2.0*framespace });
        inf_toolbar(analysis, inf_view);
        igSetCursorPos(pos_after);
        draw
    }
}

fn draw_inf(config :&Config, analysis :&Analysis, inf_view :&mut InfView, 
            dispatch_view :&Option<DispatchView>,
            draw :&Draw, preview_route :Option<usize>) {

    let instant = {
        if let Some(dref) = dispatch_view_ref(dispatch_view) {
            inf_view.instant_cache.update(analysis, dref);
            inf_view.instant_cache.get(dref)
        } else { None }
    };

    draw::base(config, analysis, inf_view, instant, dispatch_view, draw);

    if let Some(instant) = instant {
        draw::state(config, instant, inf_view, draw);
        draw::trains(config, instant, inf_view, draw);
    }

    if let Some(r) = preview_route { draw::route(config, analysis, inf_view, draw, r); }
}

fn scroll(inf_view :&mut InfView) { 
    unsafe {
        if !igIsItemHovered(0){ return; }
        let io = igGetIO();
        let wheel = (*io).MouseWheel;
        if wheel != 0.0 {
            inf_view.view.zoom(wheel);
        }
        if ((*io).KeyCtrl && igIsMouseDragging(0,-1.0)) || igIsMouseDragging(2,-1.0) {
            inf_view.view.translate((*io).MouseDelta);
        }
    }
}


fn interact(config :&Config, analysis :&mut Analysis, inf_view :&mut InfView, draw :&Draw) {
    match &inf_view.action {
        Action::Normal(normal) => { 
            let normal = *normal;
            interact_normal(config, analysis, inf_view, draw, normal); 
        },
        Action::DrawingLine(from) => { 
            let from = *from;
            interact_drawing(config, analysis, inf_view, draw, from); 
        },
        Action::InsertObject(obj) => { 
            let obj = obj.clone();
            interact_insert(config, analysis, inf_view, draw, obj); 
        },
        Action::SelectObjectType => {},
    }
}

fn interact_normal(config :&Config, analysis :&mut Analysis, 
                   inf_view :&mut InfView, draw :&Draw, state :NormalState) {
    // config
    // inf_view
    // analysis
    unsafe {
        let io = igGetIO();
        match state {
            NormalState::SelectWindow(a) => {
                let b = a + igGetMouseDragDelta_nonUDT2(0,-1.0).into();
                if igIsMouseDragging(0,-1.0) {
                    ImDrawList_AddRect(draw.draw_list, draw.pos + a, draw.pos + b,
                                       config.color_u32(RailUIColorName::CanvasSelectionWindow),
                                       0.0, 0, 1.0);
                } else {
                    set_selection_window(inf_view, analysis, a, b, (*io).KeyShift, (*io).KeyCtrl);
                    inf_view.action = Action::Normal(NormalState::Default);
                }
            },
            NormalState::DragMove(typ) => {
                if igIsMouseDragging(0,-1.0) {
                    let delta = inf_view.view.screen_to_world_ptc((*io).MouseDelta) -
                                inf_view.view.screen_to_world_ptc(ImVec2 { x:0.0, y: 0.0 });
                    
                    let (new_model, new_selection, initial_selection, final_offset) = {
                        if inf_view.drag_ghost.is_none() {
                            inf_view.drag_ghost = Some(DragState {
                                initial_model: analysis.model().clone(),
                                initial_selection: inf_view.selection.clone(),
                                offset: glm::zero(),
                            });
                        }
                        let ghost = inf_view.drag_ghost.as_mut().unwrap();

                        match typ {
                            MoveType::Continuous => {
                                ghost.offset += delta;
                            },
                            MoveType::Grid(p) => {
                                ghost.offset = p + delta;
                            },
                        }
                        
                        let (nm, ns) = apply_move_selection(&ghost.initial_model, &ghost.initial_selection, ghost.offset);
                        (nm, ns, ghost.initial_selection.clone(), ghost.offset)
                    };

                    if let MoveType::Grid(_) = typ {
                        inf_view.action = Action::Normal(NormalState::DragMove(MoveType::Grid(final_offset)));
                    }

                    analysis.set_model(new_model, Some(EditClass::MoveObjects(initial_selection)));
                    analysis.override_edit_class(EditClass::MoveObjects(new_selection.clone()));
                    inf_view.selection = new_selection;

                } else {
                    // Finalize movement
                    if let Some(_ghost) = inf_view.drag_ghost.take() {
                        // Already updated in the last dragging frame
                    }
                    inf_view.action = Action::Normal(NormalState::Default);
                }
            },
            NormalState::Default => {
                if !(*io).KeyCtrl && igIsItemHovered(0) && igIsMouseDragging(0,-1.0) {
                    if let Some((r,_)) = analysis.get_closest(
                            inf_view.view.screen_to_world_ptc(draw.mouse)) {
                        if !inf_view.selection.contains(&r) {
                            inf_view.selection = std::iter::once(r).collect();
                        }
                        if inf_view.selection.iter().any(|x| matches!(x, Ref::Node(_)) || matches!(x, Ref::LineSeg(_,_))) {
                            inf_view.action = Action::Normal(NormalState::DragMove(
                                    MoveType::Grid(glm::zero())));
                        } else {
                            inf_view.action = Action::Normal(NormalState::DragMove(MoveType::Continuous));
                        }
                    } else {
                        let a = (*io).MouseClickedPos[0] - draw.pos;
                        //let b = a + igGetMouseDragDelta_nonUDT2(0,-1.0).into();
                        inf_view.action = Action::Normal(NormalState::SelectWindow(a));
                    }
                } else {
                    if igIsItemHovered(0) && igIsMouseReleased(0) && !igIsMouseDragging(0, -1.0) {
                        if let Some((r,_)) = analysis.get_closest(
                                inf_view.view.screen_to_world_ptc(draw.mouse)) {
                            if (*io).KeyShift || (*io).KeyCtrl {
                                if inf_view.selection.contains(&r) {
                                    inf_view.selection.remove(&r);
                                } else {
                                    inf_view.selection.insert(r);
                                }
                            } else {
                                inf_view.selection.clear();
                                inf_view.selection.insert(r);
                            }
                        } else {
                            if !((*io).KeyShift || (*io).KeyCtrl) {
                                inf_view.selection.clear();
                            }
                        }
                    }
                }
            },
        }
    }

}

pub fn set_selection_window(inf_view :&mut InfView, analysis :&Analysis, a :ImVec2, b :ImVec2, shift: bool, ctrl: bool) {
    let s = analysis.get_rect(inf_view.view.screen_to_world_ptc(a),
                         inf_view.view.screen_to_world_ptc(b));
    if shift || ctrl {
        for r in s {
            inf_view.selection.insert(r);
        }
    } else {
        inf_view.selection = s.into_iter().collect();
    }
}

pub fn move_selection(analysis: &mut Analysis, inf_view: &mut InfView, delta: PtC) {
    let (model, selection) = apply_move_selection(analysis.model(), &inf_view.selection, delta);
    let selection_before = inf_view.selection.clone();
    inf_view.selection = selection;
    analysis.set_model(model, Some(EditClass::MoveObjects(selection_before)));
    analysis.override_edit_class(EditClass::MoveObjects(inf_view.selection.clone()));
}

pub fn apply_move_selection(base_model: &Model, base_selection: &std::collections::HashSet<Ref>, delta: PtC) -> (Model, std::collections::HashSet<Ref>) {
    let is_grid_locked = base_selection.iter().any(|r| matches!(r, Ref::Node(_)) || matches!(r, Ref::LineSeg(_,_)));
    let delta = if is_grid_locked {
        glm::vec2(delta.x.round(), delta.y.round())
    } else {
        delta
    };
    let mut model = base_model.clone();

    // 1. Identify all points that belong to the selection (and thus should move)
    let mut moving_points = im::HashSet::new();
    for &r in base_selection {
        match r {
            Ref::Node(p) => { moving_points.insert(p); }
            Ref::LineSeg(p1, p2) => {
                moving_points.insert(p1);
                moving_points.insert(p2);
            }
            _ => {}
        }
    }

    // 2. Map of old points to new coordinates
    let mut point_map = im::HashMap::new();
    for &p in &moving_points {
        let new_p = glm::vec2((p.x as f32 + delta.x).round() as i32, 
                             (p.y as f32 + delta.y).round() as i32);
        point_map.insert(p, new_p);
    }

    // 3. Update linesegs (DETACHMENT LOGIC)
    let mut new_linesegs = im::HashSet::new();
    for &(p1, p2) in base_model.linesegs.iter() {
        if base_selection.contains(&Ref::LineSeg(p1, p2)) {
            // This line is selected, it moves to its new endpoints
            let np1 = point_map.get(&p1).cloned().unwrap_or(p1);
            let np2 = point_map.get(&p2).cloned().unwrap_or(p2);
            if np1 != np2 { new_linesegs.insert(util::order_ivec(np1, np2)); }
        } else {
            // This line is NOT selected, it stays at its original position
            new_linesegs.insert((p1, p2));
        }
    }
    model.linesegs = new_linesegs;

    // 4. Update objects
    let mut new_objects = im::HashMap::new();
    let mut changed_ptas = Vec::new();
    for (pta, obj) in base_model.objects.iter() {
        let pta = *pta;
        if base_selection.contains(&Ref::Object(pta)) {
            let mut obj = obj.clone();
            obj.move_to(&model, obj.loc + delta);
            let new_pta = round_coord(obj.loc);
            new_objects.insert(new_pta, obj);
            if pta != new_pta { changed_ptas.push((pta, new_pta)); }
        } else {
            new_objects.insert(pta, obj.clone());
        }
    }
    model.objects = new_objects;

    // 5. Update node_data (DETACHMENT LOGIC)
    let mut new_node_data = im::HashMap::new();
    // Copy all unselected nodes (or nodes that still have unselected things attached)
    for (p, data) in base_model.node_data.iter() {
        let p = *p;
        new_node_data.insert(p, data.clone());
    }

    // Move selected node data
    for &r in base_selection {
        if let Ref::Node(p) = r {
            if let Some(data) = base_model.node_data.get(&p) {
                let np = *point_map.get(&p).unwrap_or(&p);
                // If it moved, we remove from old and put in new.
                // But wait, if it's detached, we should keep the OLD one if unselected lines still use it.
                // Since we already filled new_node_data with all original nodes,
                // we only need to "move" it if it moved.
                if np != p {
                    // Remove from old location IF it's not used by any unselected lines
                    // Actually, to make it simple: if the Node itself was selected, 
                    // the USER wants the "Node" entity to move.
                    new_node_data = new_node_data.without(&p);
                    new_node_data.insert(np, data.clone());
                }
            }
        }
    }
    model.node_data = new_node_data;

    // 6. Update references in dispatches and plans
    for &r in base_selection {
        if let Ref::Node(a) = r {
            if let Some(&b) = point_map.get(&a) {
                if a != b { model_rename_node(&mut model, a, b); }
            }
        }
    }
    for (a, b) in &changed_ptas {
        model_rename_object(&mut model, *a, *b);
    }

    // 7. Update selection to reflect new coordinates
    let mut new_selection = std::collections::HashSet::new();
    for &r in base_selection {
        match r {
            Ref::Node(p) => { 
                new_selection.insert(Ref::Node(*point_map.get(&p).unwrap_or(&p))); 
            }
            Ref::LineSeg(p1, p2) => {
                let np1 = point_map.get(&p1).unwrap_or(&p1);
                let np2 = point_map.get(&p2).unwrap_or(&p2);
                new_selection.insert(Ref::LineSeg(*np1, *np2));
            }
            Ref::Object(p) => {
                let mut final_p = p;
                for (old_pta, new_pta) in &changed_ptas {
                    if p == *old_pta { final_p = *new_pta; break; }
                }
                new_selection.insert(Ref::Object(final_p));
            }
        }
    }
    
    (model, new_selection)
}

fn interact_drawing(config :&Config, analysis :&mut Analysis, inf_view :&mut InfView, 
                    draw :&Draw, from :Option<Pt>) {
    unsafe {
        let color = config.color_u32(RailUIColorName::CanvasTrackDrawing);
        let pt_end = inf_view.view.screen_to_world_pt(draw.mouse);
        // Draw preview
        if let Some(pt) = from {
            for (p1,p2) in util::route_line(pt, pt_end) {
                ImDrawList_AddLine(draw.draw_list, draw.pos + inf_view.view.world_pt_to_screen(p1),
                                                   draw.pos + inf_view.view.world_pt_to_screen(p2),
                                              color, 2.0);
            }

            if !igIsMouseDown(0) {
                if pt != pt_end {
                    let mut new_model = analysis.model().clone();
                    if let Some((p1,p2)) = is_boundary_extension(analysis, pt, pt_end) {
                        model_rename_node(&mut new_model, p1, p2);
                    }
                    for (p1,p2) in util::route_line(pt,pt_end) {
                        let unit = util::unit_step_diag_line(p1,p2);
                        for (pa,pb) in unit.iter().zip(unit.iter().skip(1)) {
                            new_model.linesegs.insert(util::order_ivec(*pa,*pb));
                        }
                    }
                    analysis.set_model(new_model, None);
                    inf_view.selection = std::iter::empty().collect();
                }
                inf_view.action = Action::DrawingLine(None);
            }
        } else {
            if igIsItemHovered(0) && igIsMouseDown(0) {
                inf_view.action = Action::DrawingLine(Some(pt_end));
            }
        }
    }
}

fn is_boundary_extension(analysis :&Analysis, p1 :Pt, p2 :Pt) -> Option<(Pt,Pt)> {
    let locs = &analysis.data().topology.as_ref()?.1.locations;
    match (locs.get(&p1), locs.get(&p2)) {
        (Some((NDType::OpenEnd, _)), None) => { return Some((p1,p2)); }
        _ => {},
    }
    match (locs.get(&p2), locs.get(&p1)) {
        (Some((NDType::OpenEnd, _)), None) => { return Some((p2,p1)); }
        _ => {},
    }
    None
}

fn model_rename_node(model :&mut Model, a :Pt, b :Pt) {
    for (_,dispatch) in model.dispatches.iter_mut() {
        for (_,(_,command)) in dispatch.commands.iter_mut() {
            match command {
                Command::Train(_,r) | Command::Route(r) => {
                    if r.from == Ref::Node(a) {
                        r.from = Ref::Node(b);
                    }
                    if r.to == Ref::Node(a) {
                        r.to = Ref::Node(b);
                    }
                }
            };
        }
    }

    for (_,p) in model.plans.iter_mut() {
        for (_,(_veh, visits)) in p.trains.iter_mut() {
            for (_,v) in visits.iter_mut() {
                for l in v.locs.iter_mut() {
                    if l == &Ok(Ref::Node(a)) {
                        *l = Ok(Ref::Node(b));
                    }
                }
            }
        }
    }
}

fn model_rename_object(model :&mut Model, a :PtA, b :PtA) {
    for (_,dispatch) in model.dispatches.iter_mut() {
        for (_,(_,command)) in dispatch.commands.iter_mut() {
            match command {
                Command::Train(_,r) | Command::Route(r) => {
                    if r.from == Ref::Object(a) {
                        r.from = Ref::Object(b);
                    }
                    if r.to == Ref::Object(a) {
                        r.to = Ref::Object(b);
                    }
                }
            };
        }
    }

    for (_,p) in model.plans.iter_mut() {
        for (_,(_veh, visits)) in p.trains.iter_mut() {
            for (_,v) in visits.iter_mut() {
                for l in v.locs.iter_mut() {
                    if l == &Ok(Ref::Object(a)) {
                        *l = Ok(Ref::Object(b));
                    }
                }
            }
        }
    }
}


fn interact_insert(config :&Config, analysis :&mut Analysis, 
                   inf_view :&InfView, draw :&Draw, obj :Option<Object>) {
    unsafe {
        if let Some(mut obj) = obj {
            let moved = obj.move_to(analysis.model(),inf_view.view.screen_to_world_ptc(draw.mouse));
            obj.draw(draw.pos,&inf_view.view,draw.draw_list,
                     config.color_u32(RailUIColorName::CanvasSymbol),&[],&config);

            if let Some(err) = moved {
                let p = draw.pos + inf_view.view.world_ptc_to_screen(obj.loc);
                let window = ImVec2 { x: 4.0, y: 4.0 };
                ImDrawList_AddRect(draw.draw_list, p - window, p + window,
                                   config.color_u32(RailUIColorName::CanvasSymbolLocError),
                                   0.0,0,4.0);
            } else  {
                if igIsMouseReleased(0) {
                    analysis.edit_model(|m| {
                        m.objects.insert(round_coord(obj.loc), obj.clone());
                        None
                    });
                }
            }
        }
    }
}

fn inf_toolbar(analysis :&mut Analysis, inf_view :&mut InfView) {
    unsafe  {
    if toolbar_button(
        const_cstr!("\u{f245}").as_ptr(), 
                      matches!(inf_view.action, Action::Normal(_)), true) {
        inf_view.action = Action::Normal(NormalState::Default);
    }
    if igIsItemHovered(0) {
        igBeginTooltip();
        widgets::show_text("\u{f245} select (A)\nSelect tracks, nodes and objects. Drag to move.");
        igEndTooltip();
    }

    igSameLine(0.0,-1.0);

    object_select(inf_view);

    if toolbar_button(const_cstr!("\u{f637}").as_ptr(), 
                      matches!(inf_view.action, Action::InsertObject(_)) || 
                      matches!(inf_view.action, Action::SelectObjectType), true) {
        inf_view.action = Action::SelectObjectType;
    }
    if igIsItemHovered(0) {
        igBeginTooltip();
        widgets::show_text("\u{f637} insert object (S)\nOpens a drop-down menu for selecting an object type.\nInsert the object by clicking a position.");
        igEndTooltip();
    }
    igSameLine(0.0,-1.0);

    if toolbar_button(const_cstr!("\u{f303}").as_ptr(), 
                      matches!(inf_view.action, Action::DrawingLine(_)), true ) {
        inf_view.action = Action::DrawingLine(None);
    }
    if igIsItemHovered(0) {
        igBeginTooltip();
        widgets::show_text("\u{f303} draw tracks (D)\nClick and drag to create new tracks.");
        igEndTooltip();
    }
    igSameLine(0.0,-1.0);
    if toolbar_button(const_cstr!("\u{f0e2}").as_ptr(), false, analysis.can_undo()) {
        analysis.undo();
    }
    if igIsItemHovered(0) {
        igBeginTooltip();
        widgets::show_text("\u{f0e2} undo (CTRL-Z)\nUndo the previous action.");
        igEndTooltip();
    }
    igSameLine(0.0,-1.0);
    if toolbar_button(const_cstr!("\u{f01e}").as_ptr(), false, analysis.can_redo()) {
        analysis.redo();
    }
    if igIsItemHovered(0) {
        igBeginTooltip();
        widgets::show_text("\u{f01e} redo (CTRL-Y)\nRedo the previously undone action.");
        igEndTooltip();
    }
    }
}

fn toolbar_button(name :*const i8, selected :bool, enabled :bool) -> bool {
        unsafe {
        if selected {
            let c1 = ImVec4 { x: 0.4, y: 0.65,  z: 0.4, w: 1.0 };
            let c2 = ImVec4 { x: 0.5, y: 0.85, z: 0.5, w: 1.0 };
            let c3 = ImVec4 { x: 0.6, y: 0.9,  z: 0.6, w: 1.0 };
            igPushStyleColor(ImGuiCol__ImGuiCol_Button as _, c1);
            igPushStyleColor(ImGuiCol__ImGuiCol_ButtonHovered as _, c1);
            igPushStyleColor(ImGuiCol__ImGuiCol_ButtonActive as _, c1);
        }
        if !enabled {
            igPushDisable();
            igPushStyleVarFloat(ImGuiStyleVar__ImGuiStyleVar_Alpha as _, 0.5);

        }
        let clicked = igButton( name , ImVec2 { x: 0.0, y: 0.0 } );
        if !enabled {
            igPopStyleVar(1);
            igPopDisable();
        }
        if selected {
            igPopStyleColor(3);
        }
        clicked
    }
}

fn object_select(inf_view :&mut InfView) {
    unsafe {
        if matches!(&inf_view.action, Action::SelectObjectType) {
            inf_view.action = Action::InsertObject(None);
            igOpenPopup(const_cstr!("osel").as_ptr());
        }

        if igBeginPopup(const_cstr!("osel").as_ptr(), 0 as _) {


            if igSelectable(const_cstr!("Signal").as_ptr(), false, 0 as _, ImVec2::zero()) {
                inf_view.action = Action::InsertObject(Some( 
                        Object {
                            loc: glm::vec2(0.0, 0.0),
                            tangent: glm::vec2(1,0),
                            functions: vec![Function::MainSignal { has_distant: false}],
                        }
                        ));
            } 
            if igSelectable(const_cstr!("Detector").as_ptr(), false, 0 as _, ImVec2::zero()) {
                inf_view.action = Action::InsertObject(Some( 
                        Object {
                            loc: glm::vec2(0.0, 0.0),
                            tangent: glm::vec2(1,0),
                            functions: vec![Function::Detector],
                        }
                        ));
            } 

            igEndPopup();
        }
    }
}

fn context_menu(analysis :&mut Analysis, 
                inf_view :&mut InfView,
                dispatch_view :&mut Option<DispatchView>,
                draw :&Draw, preview_route :&mut Option<usize>) {
    unsafe {
    if igBeginPopup(const_cstr!("ctx").as_ptr(), 0 as _) {
        context_menu_contents(analysis, inf_view, dispatch_view, preview_route);
        igEndPopup();
    }

    if igIsItemHovered(0) && igIsMouseClicked(1, false) {
        if let Some((r,_)) = analysis.get_closest(inf_view.view.screen_to_world_ptc(draw.mouse)) {
            if !inf_view.selection.contains(&r) {
                inf_view.selection = std::iter::once(r).collect();
            }
        }
        igOpenPopup(const_cstr!("ctx").as_ptr());
    }
    }
}

fn selection_title(inf_view :&InfView) -> String {
    if inf_view.selection.len() == 0 {
        format!("No selection")
    }
    else if inf_view.selection.len() == 1 {
        match inf_view.selection.iter().next() {
            Some(Ref::LineSeg(a,b)) => format!("Line segment from ({},{}) to ({},{})", a.x, a.y, b.x, b.y),
            Some(Ref::Node(pt)) => format!("Node at ({},{})", pt.x, pt.y),
            Some(Ref::Object(pt)) => format!("Object at ({:.1},{:.1})", pt.x as f32 / 10.0, pt.y as f32 / 10.0),
            None => unreachable!(),
        }
    }
    else {
        let (mut n_linesegs, mut n_nodes, mut n_objects) = (0,0,0);
        for x in inf_view.selection.iter() {
            match x {
                Ref::LineSeg(_,_) => { n_linesegs += 1; },
                Ref::Node(_) => { n_nodes += 1; },
                Ref::Object(_) => { n_objects += 1; },
            }
        }
        if n_nodes == 0 && n_objects == 0 { format!("Selection: {} line segments.", n_linesegs) }
        else if n_linesegs == 0 && n_objects == 0 { format!("Selection: {} nodes.", n_nodes) }
        else if n_linesegs == 0 && n_nodes == 0 { format!("Selection: {} objects.", n_objects) }
        else {
            format!("Selection: {} entities.", inf_view.selection.len())
        }
    }
}

fn context_menu_contents(analysis :&mut Analysis, inf_view :&mut InfView,
                         dispatch_view :&mut Option<DispatchView>,
                         preview_route :&mut Option<usize>) {
    unsafe {
    widgets::show_text(&selection_title(inf_view));

    widgets::sep();
    if !inf_view.selection.is_empty() {
        if igSelectable(const_cstr!("Delete").as_ptr(), false, 0 as _, ImVec2::zero()) {
            delete_selection(analysis, inf_view);
        }
    }
    widgets::sep();
    if inf_view.selection.len() == 1 {
        let thing = inf_view.selection.iter().nth(0).cloned().unwrap();
        context_menu_single(analysis, dispatch_view ,thing,preview_route);
    }
    }
}

fn context_menu_single(analysis :&mut Analysis, 
                       dispatch_view :&mut Option<DispatchView>,
                       thing :Ref, preview_route :&mut Option<usize>) {

    // Node editor
    if let Ref::Node(pt) = thing { 
        menus::node_editor(analysis, pt);
        widgets::sep();
    }

    // Object editor
    if let Ref::Object(pta) = thing { 
        menus::object_menu(analysis, pta);
        widgets::sep();
    }

    // Manual dispatch from boundaries and signals
    let action = menus::route_selector(analysis, dispatch_view, thing, preview_route);
    if let Some(routespec) = action {
        start_route(analysis, dispatch_view, routespec);
    }
    widgets::sep();

    // Add visits to auto dispatch
    menus::add_plan_visit(analysis, dispatch_view, thing);
}


pub fn delete_selection(analysis :&mut Analysis, inf_view :&mut InfView) {
    let mut new_model = analysis.model().clone();
    for x in inf_view.selection.drain() {
        new_model.delete(x);
    }
    analysis.set_model(new_model, None);
}

fn start_route(analysis :&mut Analysis, dispatch_view :&mut Option<DispatchView>, cmd :Command) {
    let mut model = analysis.model().clone();

    let (dispatch_idx,time) = match &dispatch_view {
        Some(DispatchView::Manual(m)) => (m.dispatch_idx, m.time),
        None | Some(DispatchView::Auto(_)) => {
            let name = format!("Dispatch {}", model.dispatches.next_id()+1);
            let dispatch_idx = model.dispatches.insert(Dispatch::new_empty(name));
            let time = 0.0;

            let mut m = ManualDispatchView::new(dispatch_idx);
            let autoplay = true; if autoplay { m.play = true; }
            *dispatch_view = Some(DispatchView::Manual(m));
            (dispatch_idx,time)
        },
    };

    let dispatch = model.dispatches.get_mut(dispatch_idx).unwrap();
    dispatch.insert(time as f64, cmd);
    analysis.set_model(model, None);
}

fn dispatch_view_ref(dispatch_view :&Option<DispatchView>) -> Option<DispatchRef> {
    match dispatch_view {
        Some(DispatchView::Manual(ManualDispatchView { dispatch_idx, time, .. })) => {
           Some((Ok(*dispatch_idx),*time as _))
        },
        Some(DispatchView::Auto(AutoDispatchView { plan_idx,
            dispatch: Some(ManualDispatchView { dispatch_idx, time, .. }), .. })) => {
           Some((Err((*plan_idx, *dispatch_idx)), *time as _))
        },
        _ => { return None; },
    }
}

