use const_cstr::*;
use crate::document::Document;
use crate::app::{Windows, PendingAction};
use crate::gui::widgets;
use crate::file;
use log::*;

pub fn unsaved_changes_window(doc :&mut Document, show_windows :&mut Windows) -> Option<bool> {
    unsafe {
    use backend_glfw::imgui::*;
    let mut result = None;
    let action = show_windows.pending_action.unwrap();

    let name = const_cstr!("Unsaved changes").as_ptr();
    if !igIsPopupOpen(name) { igOpenPopup(name); }

    if igBeginPopupModal(name, &mut true as *mut bool, 0 as _) {
        let msg = match action {
            PendingAction::New => "Create new file? Unsaved changes will be lost.",
            PendingAction::Load => "Load file? Unsaved changes will be lost.",
            PendingAction::Import => "Import from railML? Unsaved changes will be lost.",
            PendingAction::Quit => "Quit program? Unsaved changes will be lost.",
        };
        widgets::show_text(msg);

        let yes = const_cstr!("Save").as_ptr();
        let no = const_cstr!("Discard").as_ptr();
        let cancel = const_cstr!("Cancel").as_ptr();

        if igButton(yes, ImVec2{ x: 80.0, y: 0.0 }) {
            let model = doc.analysis.model().clone();
            match file::save_interactive(model) {
                Ok(Some(filename)) => { 
                    doc.set_saved_file(filename);
                    result = Some(true); 
                },
                Ok(None) => { /* cancelled save, stay in dialog */ },
                Err(e) => { error!("Could not save file {:?}", e); },
            };
        }
        igSameLine(0.0, -1.0);
        if igButton(no, ImVec2{ x: 80.0, y: 0.0 }) {
            result = Some(true);
        }
        igSameLine(0.0, -1.0);
        if igButton(cancel, ImVec2{ x: 80.0, y: 0.0 }) {
            result = Some(false);
        }
        igEndPopup();
    }
    result
    }
}
