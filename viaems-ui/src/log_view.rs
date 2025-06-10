use std::cell::RefCell;
use std::fmt::Pointer;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;
use std::time::SystemTime;

use eframe::egui::containers::Frame;
use eframe::egui::{self, Pos2};
use egui::Rect;
use emath::RectTransform;
use epaint;

use crate::view_cache;
use egui_tiles;

#[derive(Clone)]
pub struct ViewerSeriesConfig {
    name: String,
    min: f32,
    max: f32,
    color: egui::Color32,
}

#[derive(Clone)]
pub struct ViewerPaneConfig {
    title: String,
    series: Vec<ViewerSeriesConfig>,
    locked: bool,
}
// Toplevel config for the log viewer
pub struct ViewerConfig {
    panes: Vec<ViewerPaneConfig>,
    time_start_ns: i64,
    time_stop_ns: i64,
}

impl Default for ViewerConfig {
    // Show last 5 minutes of a few important things like rpm, map
    fn default() -> ViewerConfig {
        ViewerConfig {
            panes: vec![
                ViewerPaneConfig {
                    title: "Pane 1".to_owned(),
                    series: vec![ViewerSeriesConfig {
                        name: "rpm".to_owned(),
                        min: 0.0,
                        max: 7000.0,
                        color: egui::Color32::RED,
                    }],
                    locked: true,
                },
                ViewerPaneConfig {
                    title: "Pane 2".to_owned(),
                    series: vec![
                        ViewerSeriesConfig {
                            name: "sensor.map".to_owned(),
                            min: 0.0,
                            max: 250.0,
                            color: egui::Color32::LIGHT_GREEN,
                        },
                        ViewerSeriesConfig {
                            name: "sensor.ego".to_owned(),
                            min: 0.7,
                            max: 1.4,
                            color: egui::Color32::YELLOW,
                        },
                    ],
                    locked: true,
                },
            ],
            time_start_ns: 0,
            time_stop_ns: 0,
        }
    }
}

struct Pane {
    name: String,
}

pub struct LogViewer {
    tree: egui_tiles::Tree<Pane>,
    behavior: LogViewerBehavior,
}

impl LogViewer {
    pub fn new(cache: Rc<RefCell<view_cache::ViewCache>>) -> LogViewer {
        let mut tiles = egui_tiles::Tiles::default();
        let vertical = tiles.insert_vertical_tile(vec![]);
        let tree = egui_tiles::Tree::new("logview", vertical, tiles);

        let mut result = LogViewer {
            tree,
            behavior: LogViewerBehavior {
                config: ViewerConfig::default(),
                cache,
            },
        };

        for p in &result.behavior.config.panes {
            let pane = Pane {
                name: p.title.clone(),
            };
            let tid = result.tree.tiles.insert_pane(pane);
            let root = result.tree.root.expect("root should exist");
            let bleh = result.tree.tiles.get_mut(root).unwrap();
            if let egui_tiles::Tile::Container(c) = bleh {
                c.add_child(tid);
            }
        }

        result
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        self.tree.ui(&mut self.behavior, ui);
    }
}

struct LogViewerBehavior {
    config: ViewerConfig,
    cache: Rc<RefCell<view_cache::ViewCache>>,
}

impl egui_tiles::Behavior<Pane> for LogViewerBehavior {
    fn tab_title_for_pane(&mut self, pane: &Pane) -> egui::WidgetText {
        pane.name.clone().into()
    }

    fn pane_ui(
        &mut self,
        ui: &mut egui::Ui,
        tile_id: egui_tiles::TileId,
        pane: &mut Pane,
    ) -> egui_tiles::UiResponse {
        let bgcolor = egui::Color32::BLACK;
        ui.painter().rect_filled(ui.max_rect(), 2, bgcolor);
        ui.label(&pane.name);

        let config = self
            .config
            .panes
            .iter()
            .find(|x| x.title == pane.name)
            .unwrap();
        let cache = self.cache.borrow();
        let drawrect = ui.max_rect();
        let normal_rect = egui::Rect::from_x_y_ranges(0.0..=1.0, 0.0..=1.0);
        for series in &config.series {
            cache.with_cache100(|vp| {
                if let Some(points) = vp.get(&series.name) {
                    let tf = RectTransform::from_to(normal_rect, drawrect);

                    let start = points.first().unwrap().time.start;
                    let end = points.last().unwrap().time.end;

                    println!("points: {}", points.len());
                    for ps in points.iter() {
                        let normalized_x1 = (ps.time.start - start) as f32 / (end - start) as f32;
                        let normalized_x2 = (ps.time.end - start) as f32 / (end - start) as f32;
                        let normalized_y2 = 1.0 - (ps.value.start / series.max as f32);
                        let normalized_y1 = 1.0 - (ps.value.end / series.max as f32);

                        let r = tf.transform_rect(Rect::from_x_y_ranges(
                            normalized_x1..=normalized_x2,
                            normalized_y1..=normalized_y2,
                        ));

                        ui.painter().add(epaint::Shape::rect_filled(
                            r,
                            egui::CornerRadius::ZERO,
                            series.color,
                        ));
                    }
                    if let Some(mouse_position) = ui.input(|i| i.pointer.hover_pos()) {
                        let value_at_mouse = 100.0;
                        ui.label(format!("{}: {}", series.name, value_at_mouse));
                    }
                }
            });
        }

        if let Some(mouse_position) = ui.input(|i| i.pointer.hover_pos()) {
            if drawrect.x_range().contains(mouse_position.x) {
                let stroke = egui::Stroke::new(0.5, egui::Color32::LIGHT_GRAY);

                ui.painter().add(epaint::Shape::vline(
                    mouse_position.x,
                    drawrect.min.y..=drawrect.max.y,
                    stroke,
                ));
            }
        }

        egui_tiles::UiResponse::None
    }
}
