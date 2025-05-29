#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use clap::Parser;
use eframe::egui::accesskit::Rect;
use eframe::egui::containers::Frame;
use eframe::egui::{self, Pos2};
use egui_file::FileDialog;
use emath::RectTransform;
use epaint;
use std::ops::Index;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};
use viaems::{self, connection, interface};

mod view_cache;

#[derive(Default)]
struct FeedState {
    keys: Vec<String>,
    values: Vec<interface::FeedValue>,
    update_time: Option<SystemTime>,
}

struct Application {
    target: Option<viaems::Manager>,
    log: Option<viaems::LogReader>,
    latest_feed: Arc<Mutex<FeedState>>,

    view: view_cache::ViewCache,
    file_dialog: FileDialog,

    last_update_time: SystemTime,
}

impl Application {
    fn new() -> Application {
        let feed = Arc::new(Mutex::new(FeedState::default()));
        let cwd = std::env::current_dir().ok();
        let dialog = FileDialog::open_file(cwd);
        Application {
            target: None,
            log: None,
            latest_feed: feed,
            view: view_cache::ViewCache::new(),
            file_dialog: dialog,
            last_update_time: SystemTime::now(),
        }
    }

    fn open_log(&mut self, filename: &str) {
        self.log = Some(viaems::LogReader::new(filename));
        self.view.set_logreader(viaems::LogReader::new(filename));
        println!("Opening log");
    }

    fn connect_udp(&mut self) {
        let conn = Box::new(connection::UdpConnection::new(
            "127.0.0.1:5556",
            "127.0.0.1:5555",
        ));
        //    let conn = Box::new(connection::UsbConnection::new());
        let target = viaems::Manager::new(conn);
        target.on_feed({
            let feed_state = self.latest_feed.clone();
            move |time: SystemTime, keys: &Vec<String>, values: &Vec<interface::FeedValue>| {
                Application::update_feed(&feed_state, time, keys, values)
            }
        });

        self.target = Some(target);
    }

    fn update_feed(
        feed_state: &Arc<Mutex<FeedState>>,
        _: SystemTime,
        keys: &Vec<String>,
        values: &Vec<interface::FeedValue>,
    ) {
        let mut state = feed_state.lock().unwrap();
        if state.keys.len() != keys.len() {
            state.keys = keys.clone();
        }
        state.values = values.clone();
        state.update_time = Some(SystemTime::now());
    }
}

#[derive(Parser, Debug)]
struct CliArgs {
    #[arg(short = 'f', long)]
    filename: Option<String>,
}

fn main() -> Result<(), eframe::Error> {
    env_logger::init();
    let options = eframe::NativeOptions::default();
    let args = CliArgs::parse();
    let mut state = Application::new();

    if let Some(filename) = args.filename {
        state.open_log(&filename);
    }

    eframe::run_simple_native("Viaems UI", options, move |ctx, _frame| {
        ctx.set_visuals(egui::Visuals::light());
        ctx.set_pixels_per_point(1.5);
        ctx.tessellation_options_mut(|o| o.feathering = false);
        let now = SystemTime::now();
        let render_time = now.duration_since(state.last_update_time).unwrap();
        state.last_update_time = now;

        egui::TopBottomPanel::top("Menubar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("Log", |ui| {
                    if ui.button("Open log").clicked() {
                        state.file_dialog.open();
                        ui.close_menu();
                    }
                });
                if state.file_dialog.show(ctx).selected() {
                    if let Some(path) = state.file_dialog.path() {
                        let path = path.to_path_buf();
                        state.open_log(path.to_str().unwrap());
                    }
                }
                ui.menu_button("Target", |ui| {
                    if ui.button("Open UDP").clicked() {
                        state.connect_udp();
                        ui.close_menu();
                    }
                });
            });
        });
        if state.target.is_some() {
            egui::SidePanel::left("left panel").show(ctx, |ui| {
                ui.label("Live Data");
                let state = state.latest_feed.lock().unwrap();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    egui::Grid::new("feed")
                        .num_columns(2)
                        .striped(true)
                        .show(ui, |ui| {
                            for (k, v) in state.keys.iter().zip(state.values.iter()) {
                                ui.label(k);
                                ui.label(v.to_string());
                                ui.end_row();
                            }
                        });
                });
            });
        }
        let mut point_count = 0;
        egui::CentralPanel::default().show(ctx, |ui| {
            Frame::canvas(ui.style()).show(ui, |ui| {
                state.view.with_viewport(|vp| {
                    if let Some(map_vec) = vp.data.get("sensor.map") {
                        let stroke = egui::Stroke::new(1.0, egui::Color32::GREEN);
                        let drawrect = ui.max_rect();

                        let normal_rect = egui::Rect::from_x_y_ranges(0.0..=1.0, 0.0..=1.0);
                        let tf = RectTransform::from_to(normal_rect, drawrect);

                        for (idx, range) in map_vec.iter().enumerate() {
                            if !range.exists {
                                continue;
                            }
                            let normalized_x = idx as f32 / vp.config.width as f32;
                            let normalized_min = range.min / 200.0 as f32;
                            let normalized_max = range.max / 200.0 as f32;
                            let min_point =
                                tf.transform_pos(Pos2::new(normalized_x, normalized_min));
                            let max_point =
                                tf.transform_pos(Pos2::new(normalized_x, normalized_max));

                            ui.painter()
                                .add(epaint::Shape::line(vec![min_point, max_point], stroke));
                        }
                    }
                    if let Some(rpm_vec) = vp.data.get("rpm") {
                        let stroke = egui::Stroke::new(1.0, egui::Color32::RED);
                        let drawrect = ui.max_rect();

                        let normal_rect = egui::Rect::from_x_y_ranges(0.0..=1.0, 0.0..=1.0);
                        let tf = RectTransform::from_to(normal_rect, drawrect);

                        for (idx, range) in rpm_vec.iter().enumerate() {
                            if !range.exists {
                                continue;
                            }
                            let normalized_x = idx as f32 / vp.config.width as f32;
                            let normalized_min = range.min / 6000 as f32;
                            let normalized_max = range.max / 6000 as f32;
                            let min_point =
                                tf.transform_pos(Pos2::new(normalized_x, normalized_min));
                            let max_point =
                                tf.transform_pos(Pos2::new(normalized_x, normalized_max));

                            ui.painter()
                                .add(epaint::Shape::line(vec![min_point, max_point], stroke));
                        }
                    }
                });
            });
        });
        egui::TopBottomPanel::bottom("Status").show(ctx, |ui| {
            match &state.target {
                None => ui.label("Target: Not connected"),
                Some(_) => ui.label("Target: Connected!"),
            };
            match &state.log {
                None => ui.label("Log: Not connected"),
                Some(log) => {
                    let mut log_str = format!("Log: {}", log.filename());
                    let viewstat = state.view.get_status();
                    match viewstat {
                        view_cache::LoadingStatus::Done => log_str += " Loaded",
                        view_cache::LoadingStatus::Loading { progress } => {
                            log_str += &format!(" {:.0}%", progress)
                        }
                    }
                    ui.label(log_str);
                    ui.label(format!(
                        "View: {} points in {} ms",
                        point_count,
                        render_time.as_millis()
                    ))
                }
            };
        });
        ctx.request_repaint();
    })
}
