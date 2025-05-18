#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use clap::Parser;
use eframe::egui;
use egui_file::FileDialog;
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime};
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
    let options = eframe::NativeOptions::default();

    let args = CliArgs::parse();
    let mut state = Application::new();

    if let Some(filename) = args.filename {
        state.open_log(&filename);
    }

    eframe::run_simple_native("Viaems UI", options, move |ctx, _frame| {
        ctx.set_visuals(egui::Visuals::light());
        ctx.set_pixels_per_point(1.5);

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
                    ui.label(log_str)
                }
            };
        });
        egui::CentralPanel::default().show(ctx, |ui| {});
        ctx.request_repaint();
    })
}
