use backend_glfw::imgui::*;
use const_cstr::*;
use log::*;

use crate::app::*;
use crate::document::Document;
use crate::gui;
use crate::file;
use crate::export;
use crate::gui::widgets;

pub fn load(app :&mut App) {
    match file::load_interactive() {
        Ok(Some((m, filename))) => {
            info!("Loading model from file succeeded.");
            app.document = Document::from_model(m, app.background_jobs.clone());
            app.document.fileinfo.set_saved_file(filename);
        },
        Ok(None) => {
            info!("Load file cancelled by user.");
        },
        Err(e) => {
            error!("Error loading file: {}", e);
        },
    };
}

pub fn main_menu(app :&mut App) {
    unsafe {
        if igBeginMenuBar() {

            if igBeginMenu(const_cstr!("File").as_ptr(), true) {

                // TODO warn about saving file when doing new file / load file
                if igMenuItemBool(const_cstr!("New file").as_ptr(), std::ptr::null(), false, true) {
                    app.windows.pending_action = Some(PendingAction::New);
                }

                if igMenuItemBool(const_cstr!("Load file...").as_ptr(), std::ptr::null(), false, true) {
                    app.windows.pending_action = Some(PendingAction::Load);
                }

                match &app.document.fileinfo.filename  {
                    Some(filename) => {
                        if igMenuItemBool(const_cstr!("Save").as_ptr(), 
                                          std::ptr::null(), false, true) {
                            match file::save(filename, app.document.analysis.model().clone()) {
                                Err(e) => { error!("Error saving file: {}", e); },
                                Ok(()) => { 
                                    app.document.set_saved_file(filename.clone()); 
                                },
                            };
                        }
                    },
                    None => {
                        if igMenuItemBool(const_cstr!("Save...").as_ptr(), 
                                          std::ptr::null(), false, true) {
                            match file::save_interactive(app.document.analysis.model().clone()) {
                                Err(e) => { error!("Error saving file: {}", e); },
                                Ok(Some(filename)) => { app.document.set_saved_file(filename); },
                                _ => {}, // cancelled
                            };
                        }
                    }
                }

                if igMenuItemBool(const_cstr!("Save as...").as_ptr(), std::ptr::null(), false, true) {
                    match file::save_interactive(app.document.analysis.model().clone()) {
                        Err(e) => { error!("Error saving file: {}", e); },
                        Ok(Some(filename)) => {
                            app.document.set_saved_file(filename);
                        },
                        _ => {},
                    }
                }

                widgets::sep();

                if igMenuItemBool(const_cstr!("Import from railML...").as_ptr(), std::ptr::null(), false, true) {
                    app.windows.pending_action = Some(PendingAction::Import);
                }

                if igMenuItemBool(const_cstr!("Export to railML...").as_ptr(), std::ptr::null(), false, true) {
                    if let Err(e) = export::export_railml_interactive(app.document.analysis.model()) {
                        error!("Error exporting railML: {}", e);
                    }
                }

                widgets::sep();
                if igMenuItemBool(const_cstr!("Quit").as_ptr(), 
                                  std::ptr::null(), false, true) {
                    app.windows.pending_action = Some(PendingAction::Quit);
                }

                igEndMenu();
            }
            if igBeginMenu(const_cstr!("Edit").as_ptr(), true) {
                if igMenuItemBool(const_cstr!("Edit vehicles").as_ptr(), 
                                  std::ptr::null(), app.windows.vehicles, true) {
                    app.windows.vehicles = !app.windows.vehicles;
                }
                if igMenuItemBool(const_cstr!("Signal designer").as_ptr(), 
                                  std::ptr::null(), app.windows.synthesis_window.is_some(), true) {
                    if app.windows.synthesis_window.is_none() {
                        let model = app.document.analysis.model().clone();
                        let bg = app.background_jobs.clone();
                        app.windows.synthesis_window = 
                            Some(gui::windows::synthesis::SynthesisWindow::new(model, bg));

                    }
                }
                if igMenuItemBool(const_cstr!("Delete all objects").as_ptr(), std::ptr::null(), false, true) {
                    app.document.analysis.edit_model(|m| {
                        m.objects.clear();
                        None
                    });
                }
                igEndMenu();
            }
            if igBeginMenu(const_cstr!("View").as_ptr(), true) {
                if igMenuItemBool(const_cstr!("Log window").as_ptr(), 
                                  std::ptr::null(), app.windows.log, true) {
                    app.windows.log = !app.windows.log;
                }
                if igMenuItemBool(const_cstr!("Fit to view").as_ptr(),
                                  std::ptr::null(), false, true) {
                    app.document.inf_view.pending_fit_view = true;
                }
                igEndMenu();
            }
            if igBeginMenu(const_cstr!("Tools").as_ptr(), true) {
                if igMenuItemBool(const_cstr!("View data").as_ptr(), 
                                  std::ptr::null(), app.windows.debug, true) {
                    app.windows.debug = !app.windows.debug;
                }
                if igMenuItemBool(const_cstr!("Configure colors").as_ptr(), 
                                  std::ptr::null(), app.windows.config, true) {
                    app.windows.config = !app.windows.config;
                }
                igEndMenu();
            }

            igEndMenuBar();
        }
    }
}

