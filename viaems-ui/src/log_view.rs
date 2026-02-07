use std::sync::{Arc, Mutex};

use chrono::DateTime;

use eframe::egui::{self, Pos2};
use egui::Align;
use epaint;

use crate::view_cache::{self, Range};
use egui_tiles;

#[derive(Clone)]
pub struct ViewerSeriesConfig {
    name: String,
    enabled: bool,
    min: f32,
    max: f32,
    color: egui::Color32,
}

#[derive(Clone)]
pub struct ViewerPaneConfig {
    title: String,
    series: Vec<ViewerSeriesConfig>,
    settings_open: bool,
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
                        name: "position.average_rpm".to_owned(),
                        enabled: true,
                        min: 0.0,
                        max: 7000.0,
                        color: egui::Color32::RED,
                    }],
                    settings_open: false,
                },
                ViewerPaneConfig {
                    title: "Pane 2".to_owned(),
                    series: vec![
                        ViewerSeriesConfig {
                            name: "sensors.map".to_owned(),
                            enabled: true,
                            min: 0.0,
                            max: 250.0,
                            color: egui::Color32::LIGHT_GREEN,
                        },
                        ViewerSeriesConfig {
                            name: "sensors.ego".to_owned(),
                            enabled: true,
                            min: 0.7,
                            max: 1.4,
                            color: egui::Color32::YELLOW,
                        },
                    ],
                    settings_open: false,
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

    follow_feed: bool,
}

impl LogViewer {
    pub fn new(cache: Arc<Mutex<view_cache::ViewCache>>) -> LogViewer {
        let mut tiles = egui_tiles::Tiles::default();
        let vertical = tiles.insert_vertical_tile(vec![]);
        let tree = egui_tiles::Tree::new("logview", vertical, tiles);

        let mut result = LogViewer {
            tree,
            behavior: LogViewerBehavior {
                config: ViewerConfig::default(),
                cache,
            },
            follow_feed: false,
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

    pub fn set_available_keys(&mut self, keys: &Vec<String>) {
        self.behavior.config = ViewerConfig::default();

        // First, add any new keys as disabled default ones
        for key in keys.iter() {
            for pane in &mut self.behavior.config.panes {
                if pane.series.iter().find(|s| s.name == *key).is_none() {
                    pane.series.push(ViewerSeriesConfig {
                        name: key.to_owned(),
                        enabled: false,
                        min: 0.0,
                        max: 100.0,
                        color: egui::Color32::WHITE,
                    });
                }
            }
        }

        // Then remove any series for keys we don't have
        for pane in &mut self.behavior.config.panes {
            pane.series.retain(|s| keys.contains(&s.name));
        }

        self.behavior
            .cache
            .lock()
            .unwrap()
            .set_keys(&self.current_keys());
    }

    fn current_keys(&self) -> Vec<String> {
        let mut result = vec![];
        for pane in &self.behavior.config.panes {
            for series in &pane.series {
                if !series.enabled {
                    continue;
                }
                if !result.contains(&series.name) {
                    result.push(series.name.clone());
                }
            }
        }
        result
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        ui.input_mut(|i| {
            if self.follow_feed {
                let maybe_range = self.behavior.cache.lock().unwrap().get_log_time_range();
                if let Some(range) = maybe_range {
                    let twentyago = range.max - 20_000_000_000;
                    self.set_time_range(view_cache::Range::new(twentyago, range.max));
                }
            } else if let Some(mut timerange) = self.get_time_range() {
                // Hack to prevent scroll/drag when settings panes are open
                let rect = ui.max_rect();
                if !self.configuring()
                    && rect.contains(i.pointer.interact_pos().unwrap_or_default())
                {
                    let delta = i.smooth_scroll_delta;
                    if delta.x != 0.0 || delta.y != 0.0 {
                        let zoom = -delta.y as f64 / 100.0;
                        let zoomcenter = i
                            .pointer
                            .latest_pos()
                            .and_then(|p| Some(p.x / ui.max_rect().width()))
                            .unwrap_or(0.5) as f64;
                        let shift = delta.x as f64 / 100.0;

                        let width = (timerange.max - timerange.min) as f64;
                        let shiftamt = (shift * width) as i64;

                        let new_start = shiftamt
                            + ((timerange.min as f64) - (width * zoom * zoomcenter)) as i64;
                        let new_end = shiftamt
                            + ((timerange.max as f64) + (width * zoom * (1.0 - zoomcenter))) as i64;

                        timerange = view_cache::Range::new(new_start, new_end);
                    }

                    let drag = i.pointer.delta().to_pos2();
                    if i.pointer.is_decidedly_dragging() && drag.x != 0.0 {
                        let dragratio = (-drag.x / ui.available_width()) as f64;
                        let width = (timerange.max - timerange.min) as f64;

                        let new_start = timerange.min + (width * dragratio) as i64;
                        let new_end = timerange.max + (width * dragratio) as i64;
                        timerange = view_cache::Range::new(new_start, new_end);
                    }
                }

                self.set_time_range(timerange);
            }

            if i.consume_shortcut(&egui::KeyboardShortcut::new(
                egui::Modifiers::default(),
                egui::Key::F,
            )) {
                self.follow_feed = !self.follow_feed;
            }
        });
        ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
            ui.horizontal(|ui| {
                if ui
                    .add(egui::Button::new("⛶"))
                    .on_hover_text("Show entire log")
                    .clicked()
                {
                    let maybe_range = self.behavior.cache.lock().unwrap().get_log_time_range();
                    if let Some(range) = maybe_range {
                        self.set_time_range(range);
                        self.follow_feed = false;
                    }
                }

                let follow_button = egui::Button::new("⏭").selected(self.follow_feed);
                if ui
                    .add(follow_button)
                    .on_hover_text("Update viewer time range for new data automatically")
                    .clicked()
                {
                    self.follow_feed = !self.follow_feed;
                }
            });
        });

        let before_keys = self.current_keys();
        self.tree.ui(&mut self.behavior, ui);
        // TODO hack, find a way to just trigger this from the settings itself
        if self.current_keys() != before_keys {
            self.behavior
                .cache
                .lock()
                .unwrap()
                .set_keys(&self.current_keys());
        }

        if let Some(range) = &self.behavior.config.time_range {
            let start_ns = range.min;
            let stop_ns = range.max;

            let start = DateTime::from_timestamp_nanos(start_ns);
            let stop = DateTime::from_timestamp_nanos(stop_ns);

            // Put range labels in bottom corner
            ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::default().with_cross_align(Align::LEFT), |l| {
                        l.label(start.format("%Y-%m-%d %H:%M:%S").to_string())
                    });
                    ui.with_layout(
                        egui::Layout::default().with_cross_align(Align::RIGHT),
                        |r| r.label(stop.format("%Y-%m-%d %H:%M:%S").to_string()),
                    );
                });
            });
        }
    }

    fn set_time_range(&mut self, range: Range<i64>) {
        self.behavior.config.time_range = Some(range)
    }

    fn get_time_range(&self) -> Option<Range<i64>> {
        self.behavior.config.time_range.clone()
    }

    fn configuring(&self) -> bool {
        self.behavior
            .config
            .panes
            .iter()
            .find(|p| p.settings_open)
            .is_some()
    }
}

struct LogViewerBehavior {
    config: ViewerConfig,
    cache: Arc<Mutex<view_cache::ViewCache>>,
}

impl egui_tiles::Behavior<Pane> for LogViewerBehavior {
    fn tab_title_for_pane(&mut self, pane: &Pane) -> egui::WidgetText {
        pane.name.clone().into()
    }

    fn pane_ui(
        &mut self,
        ui: &mut egui::Ui,
        _tile_id: egui_tiles::TileId,
        pane: &mut Pane,
    ) -> egui_tiles::UiResponse {
        let bgcolor = egui::Color32::BLACK;
        ui.painter().rect_filled(ui.max_rect(), 2, bgcolor);
        ui.label(&pane.name);

        let maybe_mouse_position = ui.input(|i| i.pointer.hover_pos());

        let config = self
            .config
            .panes
            .iter_mut()
            .find(|x| x.title == pane.name)
            .unwrap();
        let mut cache = self.cache.lock().unwrap();
        if self.config.time_range.is_none() {
            self.config.time_range = cache.get_log_time_range();
        }

        let drawrect = ui.max_rect();

        let settings_button = ui.add(egui::Button::new("⚙").selected(config.settings_open));
        if settings_button.clicked() {
            config.settings_open = !config.settings_open;
        }

        egui::Popup::from_response(&settings_button)
            .open_bool(&mut config.settings_open)
            .close_behavior(egui::PopupCloseBehavior::IgnoreClicks)
            .show(|ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    egui::CollapsingHeader::new("Series")
                        .default_open(true)
                        .show(ui, |ui| {
                            egui::Grid::new("series")
                                .num_columns(4)
                                .striped(true)
                                .show(ui, |ui| {
                                    for series in &mut config.series {
                                        let mut min = series.min.to_string();
                                        let mut max = series.max.to_string();
                                        ui.checkbox(&mut series.enabled, series.name.clone());
                                        ui.text_edit_singleline(&mut min);
                                        ui.text_edit_singleline(&mut max);
                                        ui.color_edit_button_srgba(&mut series.color);
                                        series.min = min.parse().unwrap_or_default();
                                        series.max = max.parse().unwrap_or_default();
                                        ui.end_row();
                                    }
                                });
                        });
                });
            });

        for series in &config.series {
            if !series.enabled {
                continue;
            }
            let stroke = egui::Stroke::new(1.0, series.color);

            if let Some(range) = self.config.time_range {
                for (xpos, maybe_ps) in cache
                    .render(range, &series.name, drawrect.width() as usize)
                    .iter()
                    .enumerate()
                {
                    if let Some(ps) = maybe_ps {
                        let normalized_y2 = 1.0 - (ps.value.min / series.max as f32);
                        let normalized_y1 = 1.0 - (ps.value.max / series.max as f32);

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
                                &format!("{}", ps.value.max)
                            } else {
                                "---"
                            };
                            ui.colored_label(
                                series.color,
                                format!("{}: {}", series.name, value_at_mouse),
                            );
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
