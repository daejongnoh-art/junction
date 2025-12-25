use matches::matches;
use serde::{Serialize, Deserialize};

use crate::document::view::*;
use crate::document::model::*;

use crate::config::*;
use crate::util::*;
use backend_glfw::imgui::*;
use nalgebra_glm as glm;


#[derive(Clone)]
#[derive(Debug)]
#[derive(Serialize,Deserialize)]
pub struct Object {
    pub loc :PtC,
    pub tangent :Vc,
    pub functions :Vec<Function>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[derive(Serialize,Deserialize)]
pub enum SignalKind {
    Main,
    Distant,
    Combined,
    Repeater,
    Shunting,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[derive(Serialize,Deserialize)]
pub enum Function {
    MainSignal { has_distant: bool, kind: SignalKind },
    Detector,
    TrackCircuitBorder,
    Derailer,
    TrainProtectionElement,
    TrainProtectionGroup,
    Balise,
    PlatformEdge,
    SpeedChange,
    LevelCrossing,
    CrossSection,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ObjectState { SignalStop, SignalProceed, DistantStop, DistantProceed }

impl Object {
    pub fn move_to(&mut self, model :&Model, pt :PtC) -> Option<()> {
        if let Some((l,_param,(d1,d2))) = model.get_closest_lineseg(pt) {
            let (pt_on_line,_param) = project_to_line(pt, glm::vec2(l.0.x as _ ,l.0.y as _ ),
                                                 glm::vec2(l.1.x as _ ,l.1.y as _ ));
            let tangent : PtC = glm::vec2(l.1.x as f32 -l.0.x as f32 ,l.1.y as f32 -l.0.y as f32);
            let normal : PtC   = glm::vec2(-tangent.y,tangent.x);
            self.tangent = glm::vec2(tangent.x.round() as _, tangent.y.round() as _);

            if self.functions.iter().find(|c| matches!(c, Function::MainSignal { .. })).is_some() {
                    let factor = if glm::angle(&(pt_on_line - pt), &normal) > glm::half_pi() {
                        1.0 } else { -1.0 };
                    let offset = 0.25*normal*factor;
                    if factor > 0.0 { self.tangent *= -1; }
                    self.loc = pt_on_line + offset;
            } else if self.functions.iter().find(|c| matches!(c,
                Function::Detector | Function::TrackCircuitBorder | Function::Derailer |
                Function::TrainProtectionElement | Function::TrainProtectionGroup | Function::Balise |
                Function::PlatformEdge | Function::SpeedChange | Function::LevelCrossing | Function::CrossSection)).is_some() {
                self.loc = pt_on_line;
            }

            None
        } else {
            self.loc = pt;
            Some(())
        }
    }

    pub fn draw(&self, pos :ImVec2, view :&View, draw_list :*mut ImDrawList, c :u32, state :&[ObjectState], config :&Config) {
        unsafe {
            let p = pos + view.world_ptc_to_screen(self.loc);
            let scale = 5.0;
            // TODO can this be simplified?
            let tangent = ImVec2 { x: scale * self.tangent.x as f32,
                                   y: scale * -self.tangent.y as f32 };
            let normal  = ImVec2 { x: scale * -self.tangent.y as f32,
                                   y: scale * -self.tangent.x as f32 };

            for f in self.functions.iter() {
                match f {
                    Function::Detector => {
                        ImDrawList_AddLine(draw_list, p - normal, p + normal, c, 2.0);
                    },
                    Function::TrackCircuitBorder => {
                        let s = scale * 0.8;
                        ImDrawList_AddRect(draw_list, p - ImVec2 { x: s, y: s },
                                           p + ImVec2 { x: s, y: s }, c, 0.0, 0, 2.0);
                    },
                    Function::Derailer => {
                        let s = scale * 0.7;
                        ImDrawList_AddLine(draw_list, p - ImVec2 { x: s, y: s },
                                           p + ImVec2 { x: s, y: s }, c, 2.0);
                        ImDrawList_AddLine(draw_list, p - ImVec2 { x: s, y: -s },
                                           p + ImVec2 { x: s, y: -s }, c, 2.0);
                    },
                    Function::TrainProtectionElement => {
                        let s = scale * 0.7;
                        let p1 = p + ImVec2 { x: 0.0, y: -s };
                        let p2 = p + ImVec2 { x: -s, y: s };
                        let p3 = p + ImVec2 { x: s, y: s };
                        ImDrawList_AddLine(draw_list, p1, p2, c, 2.0);
                        ImDrawList_AddLine(draw_list, p2, p3, c, 2.0);
                        ImDrawList_AddLine(draw_list, p3, p1, c, 2.0);
                    },
                    Function::TrainProtectionGroup => {
                        let s = scale * 0.9;
                        let p1 = p + ImVec2 { x: 0.0, y: -s };
                        let p2 = p + ImVec2 { x: -s, y: s };
                        let p3 = p + ImVec2 { x: s, y: s };
                        ImDrawList_AddTriangle(draw_list, p1, p2, p3, c, 2.0);
                    },
                    Function::Balise => {
                        ImDrawList_AddCircleFilled(draw_list, p, scale * 0.6, c, 8);
                    },
                    Function::PlatformEdge => {
                        let s = scale * 1.2;
                        ImDrawList_AddLine(draw_list, p - ImVec2 { x: s, y: 0.0 },
                                           p + ImVec2 { x: s, y: 0.0 }, c, 2.5);
                    },
                    Function::SpeedChange => {
                        let s = scale * 0.8;
                        let p1 = p + ImVec2 { x: 0.0, y: -s };
                        let p2 = p + ImVec2 { x: -s, y: s };
                        let p3 = p + ImVec2 { x: s, y: s };
                        ImDrawList_AddLine(draw_list, p1, p2, c, 2.0);
                        ImDrawList_AddLine(draw_list, p2, p3, c, 2.0);
                        ImDrawList_AddLine(draw_list, p3, p1, c, 2.0);
                    },
                    Function::LevelCrossing => {
                        let s = scale * 0.8;
                        ImDrawList_AddLine(draw_list, p - ImVec2 { x: s, y: 0.0 },
                                           p + ImVec2 { x: s, y: 0.0 }, c, 2.0);
                        ImDrawList_AddLine(draw_list, p - ImVec2 { x: 0.0, y: s },
                                           p + ImVec2 { x: 0.0, y: s }, c, 2.0);
                    },
                    Function::CrossSection => {
                        let s = scale * 0.6;
                        ImDrawList_AddRect(draw_list, p - ImVec2 { x: s, y: s },
                                           p + ImVec2 { x: s, y: s }, c, 0.0, 0, 1.5);
                    },
                    Function::MainSignal { has_distant, kind } => {
                        // base
                        ImDrawList_AddLine(draw_list, p + normal, p - normal, c, 2.0);

                        let draw_main = matches!(kind, SignalKind::Main | SignalKind::Combined | SignalKind::Repeater | SignalKind::Shunting);
                        let draw_distant = matches!(kind, SignalKind::Distant | SignalKind::Combined) || *has_distant;
                        let stem = if draw_distant { 2.0 } else { 1.0 };
                        ImDrawList_AddLine(draw_list, p, p + stem*tangent, c, 2.0);

                        for s in state.iter() {
                            match s {
                                ObjectState::SignalStop if draw_main => {
                                    let c = config.color_u32(RailUIColorName::CanvasSignalStop);
                                    ImDrawList_AddCircleFilled(draw_list, p + stem*tangent + tangent, scale, c, 8);
                                },
                                ObjectState::SignalProceed if draw_main => {
                                    let c = config.color_u32(RailUIColorName::CanvasSignalProceed);
                                    ImDrawList_AddCircleFilled(draw_list, p + stem*tangent + tangent, scale, c, 8);
                                },
                                ObjectState::DistantStop if draw_distant => {
                                    let c = config.color_u32(RailUIColorName::CanvasSignalStop);
                                    ImDrawList_AddCircleFilled(draw_list, p + 1.5*tangent + normal, scale*0.8, c, 8);
                                },
                                ObjectState::DistantProceed if draw_distant => {
                                    let c = config.color_u32(RailUIColorName::CanvasSignalProceed);
                                    ImDrawList_AddCircleFilled(draw_list, p + 1.5*tangent + normal, scale*0.8, c, 8);
                                },
                                _ => {},
                            };
                        }

                        // distant
                        if draw_distant {
                            ImDrawList_AddCircle(draw_list, p + 1.5*tangent + normal, scale*0.8, c, 8, 2.0);
                        }
                        // main signal
                        if draw_main {
                            match kind {
                                SignalKind::Shunting => {
                                    let s = scale * 0.8;
                                    ImDrawList_AddRect(draw_list,
                                        p + stem*tangent + tangent - ImVec2 { x: s, y: s },
                                        p + stem*tangent + tangent + ImVec2 { x: s, y: s },
                                        c, 0.0, 0, 2.0);
                                }
                                SignalKind::Repeater => {
                                    ImDrawList_AddCircle(draw_list, p + stem*tangent + tangent, scale*0.7, c, 8, 2.0);
                                }
                                _ => {
                                    ImDrawList_AddCircle(draw_list, p + stem*tangent + tangent, scale, c, 8, 2.0);
                                }
                            }
                        }
                    },
                }

            }
        }
    }
}



