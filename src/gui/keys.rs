use crate::app::{App, PendingAction};
use crate::document::objects::*;
use crate::document::infview::*;
use crate::gui::infrastructure;
use crate::gui::mainmenu;
use crate::util;
use crate::file;
use crate::document::{Document, model::Ref, DispatchView, AutoDispatchView};

use log::*;
use backend_glfw::imgui::*;
use nalgebra_glm as glm;
use std::collections::HashSet;

pub fn keys(app :&mut App) {
    unsafe {
        let io = igGetIO();


        if (*io).KeyCtrl && !(*io).KeyShift && igIsKeyPressed('Z' as _, false) {
            app.document.analysis.undo();
        }
        if (*io).KeyCtrl && (*io).KeyShift && igIsKeyPressed('Z' as _, false) {
            app.document.analysis.redo();
        }
        if (*io).KeyCtrl && !(*io).KeyShift && igIsKeyPressed('Y' as _, false) {
            app.document.analysis.redo();
        }

        if (*io).KeyCtrl && igIsKeyPressed('S' as _, false) {
            match (&app.document.fileinfo.filename, (*io).KeyShift) {
                (None,_) | (_,true) => {
                    match file::save_interactive(app.document.analysis.model().clone()) {
                        Err(e) => { error!("Error saving file: {}", e); },
                        Ok(Some(filename)) => { app.document.set_saved_file(filename); },
                        _ => {},
                    }
                }
                (Some(filename),_) => {
                    match file::save(filename, app.document.analysis.model().clone()) {
                        Err(e) => { error!("Error saving file: {}", e); },
                        Ok(()) => { app.document.set_saved_file(filename.clone()); },
                        _ => {},
                    }
                },
            }
        }

        if (*io).KeyCtrl && !(*io).KeyShift && igIsKeyPressed('O' as _, false) {
            app.windows.pending_action = Some(PendingAction::Load);
        }

        if (*io).KeyCtrl && igIsKeyPressed('A' as _, false) {
            let mut selection = HashSet::new();
            let model = app.document.analysis.model();
            for l in &model.linesegs { selection.insert(Ref::LineSeg(l.0, l.1)); }
            for pt in model.node_data.keys() { selection.insert(Ref::Node(*pt)); }
            for pta in model.objects.keys() { selection.insert(Ref::Object(*pta)); }
            app.document.inf_view.selection = selection;
        }

        if (*io).KeyCtrl && igIsKeyPressed('C' as _, false) {
            let inf_view = &mut app.document.inf_view;
            let model = app.document.analysis.model();
            inf_view.clipboard = crate::document::model::Model::empty();
            let mut node_set = HashSet::new();
            for r in &inf_view.selection {
                match r {
                    Ref::Node(p) => { 
                        if let Some(data) = model.node_data.get(p) {
                            inf_view.clipboard.node_data.insert(*p, data.clone());
                            node_set.insert(*p);
                        }
                    }
                    Ref::LineSeg(p1, p2) => {
                        inf_view.clipboard.linesegs.insert((*p1, *p2));
                        node_set.insert(*p1);
                        node_set.insert(*p2);
                    }
                    Ref::Object(pta) => {
                        if let Some(obj) = model.objects.get(pta) {
                            inf_view.clipboard.objects.insert(*pta, obj.clone());
                        }
                    }
                }
            }
            // Ensure all required nodes for linesegs/objects are in node_data
            for p in node_set {
                if !inf_view.clipboard.node_data.contains_key(&p) {
                    if let Some(data) = model.node_data.get(&p) {
                        inf_view.clipboard.node_data.insert(p, data.clone());
                    }
                }
            }
        }

        if (*io).KeyCtrl && igIsKeyPressed('V' as _, false) {
            let mouse_world = app.document.inf_view.view.screen_to_world_ptc(igGetMousePos_nonUDT2().into());
            let clipboard = app.document.inf_view.clipboard.clone();
            
            // Calculate center of clipboard
            let mut pts = Vec::new();
            for p in clipboard.node_data.keys() { pts.push(glm::vec2(p.x as f32, p.y as f32)); }
            for obj in clipboard.objects.values() { pts.push(obj.loc); }
            
            if !pts.is_empty() {
                let mut avg_loc = glm::vec2(0.0, 0.0);
                for p in &pts { avg_loc += *p; }
                avg_loc /= pts.len() as f32;
                
                let delta = mouse_world - avg_loc;
                let grid_delta = glm::vec2(delta.x.round(), delta.y.round());
                
                let mut new_selection = HashSet::new();
                app.document.analysis.edit_model(|m| {
                    let mut node_map = std::collections::HashMap::new();
                    
                    // 1. Nodes
                    for (p, data) in clipboard.node_data.iter() {
                        let np = glm::vec2(p.x + grid_delta.x as i32, p.y + grid_delta.y as i32);
                        m.node_data.insert(np, data.clone());
                        node_map.insert(*p, np);
                        new_selection.insert(Ref::Node(np));
                    }
                    
                    // 2. Linesegs
                    for (p1, p2) in clipboard.linesegs.iter() {
                        let np1 = node_map.get(p1).cloned().unwrap_or(glm::vec2(p1.x + grid_delta.x as i32, p1.y + grid_delta.y as i32));
                        let np2 = node_map.get(p2).cloned().unwrap_or(glm::vec2(p2.x + grid_delta.x as i32, p2.y + grid_delta.y as i32));
                        m.linesegs.insert(util::order_ivec(np1, np2));
                        new_selection.insert(Ref::LineSeg(np1, np2));
                    }
                    
                    // 3. Objects
                    for obj in clipboard.objects.values() {
                        let mut obj = obj.clone();
                        obj.loc += grid_delta;
                        let npta = round_coord(obj.loc);
                        m.objects.insert(npta, obj);
                        new_selection.insert(Ref::Object(npta));
                    }
                    
                    None
                });
                app.document.inf_view.selection = new_selection;
            }
        }

        if igIsKeyPressed(igGetKeyIndex(ImGuiKey__ImGuiKey_Delete as _), false) {
            infrastructure::delete_selection(&mut app.document.analysis, &mut app.document.inf_view);
        }

        // Keyboard Movement (Arrow Keys)
        if !app.document.inf_view.selection.is_empty() {
            let mut delta = glm::vec2(0.0, 0.0);
            if igIsKeyPressed(igGetKeyIndex(ImGuiKey__ImGuiKey_LeftArrow as _), true) { delta.x -= 1.0; }
            if igIsKeyPressed(igGetKeyIndex(ImGuiKey__ImGuiKey_RightArrow as _), true) { delta.x += 1.0; }
            if igIsKeyPressed(igGetKeyIndex(ImGuiKey__ImGuiKey_UpArrow as _), true) { delta.y += 1.0; }
            if igIsKeyPressed(igGetKeyIndex(ImGuiKey__ImGuiKey_DownArrow as _), true) { delta.y -= 1.0; }
            
            if delta != glm::vec2(0.0, 0.0) {
                infrastructure::move_selection(&mut app.document.analysis, &mut app.document.inf_view, delta);
            }
        }

        if !igIsAnyItemActive() {
            if igIsKeyPressed('A' as _, false) {
                app.document.inf_view.action = Action::Normal(NormalState::Default);
            }

            if igIsKeyPressed(' ' as _, false) {
                if let Some(DispatchView::Manual(m)) 
                     | Some(DispatchView::Auto(AutoDispatchView { dispatch: Some(m), .. })) 
                         = &mut app.document.dispatch_view {
                    m.play = !m.play;
                }
            }

            if igIsKeyPressed('D' as _, false) {
                app.document.inf_view.action = Action::DrawingLine(None);
            }

            if igIsKeyPressed('S' as _, false) {
                app.document.inf_view.action = Action::SelectObjectType;
            }
        }
    }
}
