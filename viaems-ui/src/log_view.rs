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
    time_range: Option<Range<i64>>,
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
            time_range: None,
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

    pub fn set_time_range(&mut self, range: Range<i64>) {
        self.behavior.config.time_range = Some(range)
    }

    pub fn get_time_range(&self) -> Option<Range<i64>> {
        self.behavior.config.time_range.clone()
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

        let maybe_mouse_position = ui.input(|i| i.pointer.hover_pos());

        let config = self
            .config
            .panes
            .iter()
            .find(|x| x.title == pane.name)
            .unwrap();
        let mut cache = self.cache.borrow_mut();
        if self.config.time_range.is_none() {
            self.config.time_range = cache.get_time_range();
        }

        let drawrect = ui.max_rect();

        for series in &config.series {
            let stroke = egui::Stroke::new(1.0, series.color);

            if let Some(range) = &self.config.time_range {
                for (xpos, maybe_ps) in cache
                    .render(
                        range.start..range.end,
                        &series.name,
                        drawrect.width() as usize,
                    )
                    .iter()
                    .enumerate()
                {
                    if let Some(ps) = maybe_ps {
                        let normalized_y2 = 1.0 - (ps.value.start / series.max as f32);
                        let normalized_y1 = 1.0 - (ps.value.end / series.max as f32);

                        let p1 = Pos2 {
                            x: drawrect.x_range().min + xpos as f32,
                            y: drawrect.y_range().min + normalized_y1 * drawrect.height(),
                        };
                        let p2 = Pos2 {
                            x: drawrect.x_range().min + xpos as f32,
                            y: drawrect.y_range().min + normalized_y2 * drawrect.height(),
                        };

                        ui.painter()
                            .add(epaint::Shape::line_segment([p1, p2], stroke));
                    }
                    if let Some(pos) = maybe_mouse_position {
                        if drawrect.x_range().contains(pos.x)
                            && ((pos.x - drawrect.x_range().min) as usize == xpos)
                        {
                            let value_at_mouse = if let Some(ps) = maybe_ps {
                                &format!("{}", ps.value.end)
                            } else {
                                "---"
                            };
                            ui.label(format!("{}: {}", series.name, value_at_mouse));
                        }
                    }
                }
            }
        }
        if let Some(mouse_position) = maybe_mouse_position {
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
