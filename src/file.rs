use crate::document::model::Model;
use crate::util::order_ivec;
use std::fs::File;
use log::*;
use serde::Serialize;

pub fn load(filename :&str) -> Result<Model, std::io::Error> {
    let m = serde_cbor::from_reader(File::open(&filename)?)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    Ok(m)
}

pub fn save(filename :&str, m :Model) -> Result<(),std::io::Error> {
    info!("Will save file to file name {:?}", filename);
    serde_cbor::to_writer(&File::create(filename)?, &m)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    Ok(())
}

pub fn dump_json(filename: &str, m: &Model) -> Result<(), std::io::Error> {
    let dump = DumpModel::from_model(m);
    serde_json::to_writer_pretty(&File::create(filename)?, &dump)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    Ok(())
}

pub fn save_interactive(m :Model) -> Result<Option<String>,std::io::Error> {
    if let Some(filename) = tinyfiledialogs::save_file_dialog("Save model to file", "") {
        save(&filename, m).map(|_| Some(filename))
    } else {
        info!("User cancelled save");
        Ok(None) // user cancelled, this is not an error
    }
}

pub fn load_interactive() -> Result<Option<(Model,String)>, std::io::Error> {
    if let Some(filename) = tinyfiledialogs::open_file_dialog("Open model from file", "", None) {
        info!("Loading file from {:?}", filename);
        let m = load(&filename)?;
        Ok(Some((m,filename)))
    } else {
        Ok(None)
    }
}

#[derive(Serialize)]
struct DumpModel {
    linesegs: Vec<DumpLineSeg>,
    objects: Vec<DumpObject>,
    node_data: Vec<DumpNodeData>,
    vehicles: Vec<(usize, crate::document::model::Vehicle)>,
    dispatches: Vec<(usize, crate::document::model::Dispatch)>,
    plans: Vec<(usize, crate::document::model::PlanSpec)>,
    railml_metadata: Option<railmlio::model::Metadata>,
    railml_track_groups: Vec<railmlio::model::TrackGroup>,
    railml_ocps: Vec<railmlio::model::Ocp>,
    railml_states: Vec<railmlio::model::State>,
    railml_tracks: Vec<crate::document::model::RailMLTrackInfo>,
    railml_objects: Vec<DumpRailMLObjectEntry>,
}

#[derive(Serialize, Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct DumpPt {
    x: i32,
    y: i32,
}

impl From<crate::document::model::Pt> for DumpPt {
    fn from(pt: crate::document::model::Pt) -> Self {
        DumpPt { x: pt.x, y: pt.y }
    }
}

#[derive(Serialize)]
struct DumpLineSeg {
    a: DumpPt,
    b: DumpPt,
}

#[derive(Serialize)]
struct DumpObject {
    key: DumpPt,
    object: crate::document::objects::Object,
}

#[derive(Serialize)]
struct DumpNodeData {
    key: DumpPt,
    value: crate::document::model::NDType,
}

#[derive(Serialize)]
struct DumpRailMLObjectEntry {
    key: DumpPt,
    value: Vec<crate::document::model::RailMLObjectInfo>,
}

impl DumpModel {
    fn from_model(m: &Model) -> Self {
        let mut linesegs: Vec<DumpLineSeg> = m
            .linesegs
            .iter()
            .map(|(a, b)| {
                let (a, b) = order_ivec(*a, *b);
                DumpLineSeg {
                    a: DumpPt::from(a),
                    b: DumpPt::from(b),
                }
            })
            .collect();
        linesegs.sort_by(|l, r| (l.a, l.b).cmp(&(r.a, r.b)));

        let mut objects: Vec<DumpObject> = m
            .objects
            .iter()
            .map(|(k, v)| DumpObject {
                key: DumpPt::from(*k),
                object: v.clone(),
            })
            .collect();
        objects.sort_by(|l, r| l.key.cmp(&r.key));

        let mut node_data: Vec<DumpNodeData> = m
            .node_data
            .iter()
            .map(|(k, v)| DumpNodeData {
                key: DumpPt::from(*k),
                value: *v,
            })
            .collect();
        node_data.sort_by(|l, r| l.key.cmp(&r.key));

        let mut railml_objects: Vec<DumpRailMLObjectEntry> = m
            .railml_objects
            .iter()
            .map(|(k, v)| DumpRailMLObjectEntry {
                key: DumpPt::from(*k),
                value: v.clone(),
            })
            .collect();
        railml_objects.sort_by(|l, r| l.key.cmp(&r.key));

        DumpModel {
            linesegs,
            objects,
            node_data,
            vehicles: m.vehicles.data().iter().cloned().collect(),
            dispatches: m.dispatches.data().iter().cloned().collect(),
            plans: m.plans.data().iter().cloned().collect(),
            railml_metadata: m.railml_metadata.clone(),
            railml_track_groups: m.railml_track_groups.clone(),
            railml_ocps: m.railml_ocps.clone(),
            railml_states: m.railml_states.clone(),
            railml_tracks: m.railml_tracks.clone(),
            railml_objects,
        }
    }
}


#[derive(Debug)]
#[derive(Clone)]
pub struct FileInfo {
    pub filename :Option<String>,
    pub unsaved :bool,
}

impl FileInfo {
    pub fn empty() -> Self {
        FileInfo {
            filename :None,
            unsaved :false,
        }
    }

    pub fn set_saved_file(&mut self, filename :String) {
        self.unsaved = false;
        self.filename = Some(filename);
        self.update_window_title();
    }

    pub fn set_saved(&mut self) {
        self.unsaved = false;
        self.update_window_title();
    }

    pub fn set_unsaved(&mut self) {
        if !self.unsaved {
            self.unsaved = true;
            self.update_window_title();
        }
    }

    pub fn update_window_title(&self) {
        backend_glfw::set_window_title(&self.window_title());
    }

    pub fn window_title(&self) -> String {
        format!("{}{} - Junction", if self.unsaved {"*"}  else { "" },
                                   self.filename.as_ref().map(|x| x.as_str()).unwrap_or("Untitled"))
    }
}
