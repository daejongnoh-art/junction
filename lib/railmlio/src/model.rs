#![allow(dead_code)]


use crate::topo::Side;

//
// original railml model (simplified)
//

pub type Id = String;
pub type IdRef = String;

#[derive(Debug)]
pub struct RailML {
    pub metadata: Option<Metadata>,
    pub infrastructure: Option<Infrastructure>,
}

#[derive(Debug)]
pub struct Metadata {
    pub dc_format: Option<String>,
    pub dc_identifier: Option<String>,
    pub dc_source: Option<String>,
    pub dc_title: Option<String>,
    pub dc_language: Option<String>,
    pub dc_creator: Option<String>,
    pub dc_description: Option<String>,
    pub dc_rights: Option<String>,
    pub organizational_units: Vec<OrganizationalUnit>,
}

#[derive(Debug)]
pub struct OrganizationalUnit {
    pub id: Id,
    pub code: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Infrastructure {
    pub tracks: Vec<Track>,
    pub track_groups: Vec<TrackGroup>,
    pub ocps: Vec<Ocp>,
    pub states: Vec<State>,
}

#[derive(Debug, Clone)]
pub struct TrackGroup {
    pub id: Id,
    pub name: Option<String>,
    pub infrastructure_manager_ref: Option<String>,
    pub line_category: Option<String>,
    pub line_type: Option<String>,
    pub track_refs: Vec<TrackRef>,
}

#[derive(Debug, Clone)]
pub struct TrackRef {
    pub r#ref: IdRef,
    pub sequence: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct Ocp {
    pub id: Id,
    pub name: Option<String>,
    pub r#type: Option<String>,
}

#[derive(Debug, Clone)]
pub struct State {
    pub id: Id,
    pub disabled: Option<bool>,
    pub status: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Track {
    pub id: Id,
    pub code: Option<String>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub track_type: Option<String>,
    pub main_dir: Option<String>,
    pub begin: Node,
    pub end: Node,
    pub switches: Vec<Switch>,
    pub track_elements: TrackElements,
    pub objects: Objects,
}

#[derive(Debug, Clone)]
pub struct TrackElements {
    pub platform_edges: Vec<PlatformEdge>,
    pub speed_changes: Vec<SpeedChange>,
    pub level_crossings: Vec<LevelCrossing>,
    pub cross_sections: Vec<CrossSection>,
}

impl TrackElements {
    pub fn empty() -> Self {
        Self {
            platform_edges: Vec::new(),
            speed_changes: Vec::new(),
            level_crossings: Vec::new(),
            cross_sections: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlatformEdge {
    pub id: Id,
    pub name: Option<String>,
    pub pos: Position,
    pub dir: TrackDirection,
    pub side: Option<String>,
    pub height: Option<f64>,
    pub length: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct SpeedChange {
    pub id: Id,
    pub pos: Position,
    pub dir: TrackDirection,
    pub vmax: Option<String>,
    pub signalised: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct LevelCrossing {
    pub id: Id,
    pub pos: Position,
    pub protection: Option<String>,
    pub angle: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct CrossSection {
    pub id: Id,
    pub name: Option<String>,
    pub ocp_ref: Option<String>,
    pub pos: Position,
    pub section_type: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Node {
    pub id: Id,
    pub pos: Position,
    pub connection: TrackEndConnection,
}

#[derive(Debug, Clone)]
pub enum TrackEndConnection {
    Connection(Id, IdRef),
    BufferStop,
    OpenEnd,
    MacroscopicNode(String),
}

#[derive(Debug, Clone)]
pub enum Switch {
    Switch {
        id: Id,
        pos: Position,
        name: Option<String>,
        description: Option<String>,
        length: Option<f64>,
        connections: Vec<SwitchConnection>,
        track_continue_course: Option<SwitchConnectionCourse>,
        track_continue_radius: Option<f64>,
    },
    Crossing {
        id: Id,
        pos: Position,

        track_continue_course: Option<SwitchConnectionCourse>,
        track_continue_radius: Option<f64>,
        normal_position: Option<SwitchConnectionCourse>,

        length: Option<f64>,
        connections: Vec<SwitchConnection>,
    },
}

#[derive(Copy, Clone)]
#[derive(Debug)]
pub enum SwitchConnectionCourse {
    Straight,
    Left,
    Right,
}

impl SwitchConnectionCourse {
    pub fn opposite(&self) -> Option<SwitchConnectionCourse> {
        match self {
            SwitchConnectionCourse::Left => Some(SwitchConnectionCourse::Right),
            SwitchConnectionCourse::Right => Some(SwitchConnectionCourse::Left),
            _ => None,
        }
    }

    pub fn to_side(&self) -> Option<Side> {
        match self {
            SwitchConnectionCourse::Left => Some(Side::Left),
            SwitchConnectionCourse::Right => Some(Side::Right),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ConnectionOrientation {
    Incoming,
    Outgoing,
    RightAngled,
    Unknown,
    Other,
}

#[derive(Debug, Clone)]
pub struct SwitchConnection {
    pub id: Id,
    pub r#ref: IdRef,
    pub orientation: ConnectionOrientation,
    pub course: Option<SwitchConnectionCourse>,
    pub radius: Option<f64>,
    pub max_speed: Option<f64>,
    pub passable: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct Position {
    pub offset: f64,
    pub mileage: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct Objects {
    pub signals: Vec<Signal>,
    pub balises: Vec<Balise>,
    pub train_detectors: Vec<TrainDetector>,
    pub track_circuit_borders: Vec<TrackCircuitBorder>,
    pub derailers: Vec<Derailer>,
    pub train_protection_elements: Vec<TrainProtectionElement>,
    pub train_protection_element_groups: Vec<TrainProtectionElementGroup>,
}

impl Objects {
    pub fn empty() -> Objects {
        Objects {
            signals: Vec::new(),
            balises: Vec::new(),
            train_detectors: Vec::new(),
            track_circuit_borders: Vec::new(),
            derailers: Vec::new(),
            train_protection_elements: Vec::new(),
            train_protection_element_groups: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Signal {
    pub id: Id,
    pub pos: Position,
    pub name: Option<String>,
    pub dir: TrackDirection,
    pub sight: Option<f64>,
    pub r#type: SignalType,
    pub function: Option<SignalFunction>,
    pub code: Option<String>,
    pub switchable: Option<bool>,
    pub ocp_station_ref: Option<String>,
}

#[derive(Debug, Copy, Clone)]
pub enum SignalType {
    Main,
    Distant,
    Repeater,
    Combined,
    Shunting,
}

#[derive(Debug, Copy, Clone)]
pub enum SignalFunction {
    Exit,
    Home,
    Blocking,
    Intermediate,
    Other,
}

#[derive(Debug, Copy, Clone)]
pub enum TrackDirection {
    Up,
    Down,
}

#[derive(Debug, Clone)]
pub struct Balise {
    pub id: Id,
    pub pos: Position,
    pub name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TrainDetector {
    pub id: Id,
    pub pos: Position,
    pub axle_counting: Option<bool>,
    pub direction_detection: Option<bool>,
    pub medium: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TrackCircuitBorder {
    pub id: Id,
    pub pos: Position,
    pub insulated_rail: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Derailer {
    pub id: Id,
    pub pos: Position,
    pub dir: Option<TrackDirection>,
    pub derail_side: Option<String>,
    pub code: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TrainProtectionElement {
    pub id: Id,
    pub pos: Position,
    pub dir: Option<TrackDirection>,
    pub medium: Option<String>,
    pub system: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TrainProtectionElementGroup {
    pub id: Id,
    pub element_refs: Vec<IdRef>,
}


