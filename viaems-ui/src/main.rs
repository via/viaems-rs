#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use clap::Parser;
use egui_file::FileDialog;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use viaems::{self, connection, interface};

mod config_pane;
mod live_status;
mod log_view;
mod table_editor;
mod view_cache;
//mod config_section;

#[derive(Default)]
struct FeedState {
    update: interface::EngineUpdate,
    update_time: Option<SystemTime>,
}

struct Application {
    target: Option<viaems::Manager>,
    log: Option<viaems::Log>,
    latest_feed: Arc<Mutex<FeedState>>,
    live_configuration: Arc<Mutex<Option<interface::Configuration>>>,
    desired_configuration: Option<interface::Configuration>,
    view_cache: Arc<Mutex<view_cache::ViewCache>>,
    logview: log_view::LogViewer,
    last_update_time: SystemTime,

    open_logfile_dialog: FileDialog,
    save_config_dialog: FileDialog,
    load_config_dialog: FileDialog,
}

impl<'a> Application {
    fn new() -> Application {
        let feed = Arc::new(Mutex::new(FeedState::default()));
        let cache = Arc::new(Mutex::new(view_cache::ViewCache::new()));

        Application {
            target: None,
            log: None,
            latest_feed: feed,
            live_configuration: Arc::new(Mutex::new(None)),
            desired_configuration: None,
            logview: log_view::LogViewer::new(cache.clone()),
            view_cache: cache,
            open_logfile_dialog: FileDialog::open_file(),
            save_config_dialog: FileDialog::save_file(),
            load_config_dialog: FileDialog::open_file(),
            last_update_time: SystemTime::now(),
        }
    }

    fn set_target(&mut self, conn: connection::Connection) {
        let target = viaems::Manager::new(conn);
        target.on_update({
            let feed_state = self.latest_feed.clone();
            let view = self.view_cache.clone();
            let logwriter: Option<viaems::UpdateWriter> = if let Some(reader) = &self.log {
                let writer = reader.get_writer().expect("Unable to create log writer");
                self.logview
                    .set_available_keys(&reader.keys().unwrap_or_default());
                Some(writer)
            } else {
                None
            };
            move |time: SystemTime, update: &interface::EngineUpdate| {
                if let Some(w) = &logwriter {
                    w.add(time, update.clone());
                }
                view.lock().unwrap().add_new_data(time, update);
                Application::update_feed(&feed_state, time, update);
            }
        });

        let req = interface::Request {
            id: 1,
            request: Some(interface::request::Request::Getconfig(
                interface::request::GetConfig {},
            )),
        };
        target.command(req, {
            let latest_config = self.live_configuration.clone();
            move |resp| {
                if let Some(response) = resp.response
                    && let interface::response::Response::Getconfig(config) = response
                {
                    *latest_config.lock().unwrap() = config.config;
                }
            }
        });

        self.target = Some(target);
    }

    fn open_log(&mut self, filename: &str) {
        let log = viaems::Log::new(filename);

        self.view_cache
            .lock()
            .unwrap()
            .set_logreader(log.try_clone().expect("Unable to create logview reader"));
        self.logview
            .set_available_keys(&log.keys().unwrap_or_default());
        self.log = Some(log);
        println!("Opening log");
    }

    fn connect_udp(&mut self) {}

    fn connect_usb(&mut self) {
        let conn = connection::Connection::new_usb();
        self.set_target(conn);
    }

    fn connect_exec(&mut self) {
        let conn = connection::Connection::new_exec("/home/via/dev/viaems/obj/hosted/bleh.sh");
        self.set_target(conn);
    }

    fn update_feed(
        feed_state: &Arc<Mutex<FeedState>>,
        _: SystemTime,
        update: &interface::EngineUpdate,
    ) {
        let mut state = feed_state.lock().unwrap();
        state.update = update.clone();
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
    } else {
        state.open_log(":memory:");
    }

    eframe::run_simple_native("Viaems UI", options, move |ctx, _frame| {
        ctx.set_pixels_per_point(1.5);

        let now = SystemTime::now();
        let render_time = now.duration_since(state.last_update_time).unwrap();
        let mut should_reload = false;
        state.last_update_time = now;

        if state.open_logfile_dialog.show(ctx).selected() {
            if let Some(path) = state.open_logfile_dialog.path() {
                let path = path.to_path_buf();
                state.open_log(path.to_str().unwrap());
            }
        }

        if state.save_config_dialog.show(ctx).selected() {
            if let Some(path) = state.save_config_dialog.path()
                && let Ok(mut file) = std::fs::File::create(path)
                && let Some(config) = &state.desired_configuration
            {
                let encoded = prost::Message::encode_to_vec(config);
                file.write_all(encoded.as_slice())
                    .expect("Failed to write config");
            }
        }

        if state.load_config_dialog.show(ctx).selected() {
            if let Some(path) = state.load_config_dialog.path()
                && let Ok(mut file) = std::fs::File::open(path)
            {
                let mut contents = vec![];
                file.read_to_end(&mut contents).unwrap();
                match prost::Message::decode(contents.as_slice()) {
                    Err(e) => println!("Failed to parse config: {}", e),
                    Ok(config) => state.desired_configuration = Some(config),
                }
            }
        }

        egui::TopBottomPanel::top("Menubar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("Log", |ui| {
                    if ui.button("Open log").clicked() {
                        state.open_logfile_dialog.open();
                        ui.close();
                    }
                });
                ui.menu_button("Target", |ui| {
                    if ui.button("Open UDP").clicked() {
                        state.connect_udp();
                        ui.close();
                    }
                    if ui.button("Open USB").clicked() {
                        state.connect_usb();
                        ui.close();
                    }
                    if ui.button("Open Sim").clicked() {
                        state.connect_exec();
                        ui.close();
                    }
                });
                ui.menu_button("Config", |ui| {
                    if ui.button("Import config").clicked() {
                        state.load_config_dialog.open();
                        ui.close();
                    }
                    if state.desired_configuration.is_some() {
                        if ui.button("Export config").clicked() {
                            state.save_config_dialog.open();
                            ui.close();
                        }
                    }
                });
            });
        });
        if state.target.is_some() {
            should_reload = true;
            egui::SidePanel::left("left panel")
                .min_width(250.0)
                .show(ctx, |ui| {
                    let latest_feed = state.latest_feed.lock().unwrap().update.clone();
                    live_status::render_status_pane(ui, &latest_feed);
                });
        }

        if let Some(live_config) = &*state.live_configuration.lock().unwrap()
            && state.desired_configuration.is_none()
        {
            // If we just connected to a target, lets reset our desired state
            state.desired_configuration = Some(live_config.clone());
        }
        if let Some(config) = &mut state.desired_configuration {
            egui::SidePanel::right("right panel").show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        if let Some(target) = &state.target {
                            let req = interface::Request {
                                id: 1,
                                request: Some(interface::request::Request::Setconfig(
                                    interface::request::SetConfig {
                                        config: Some(config.clone()),
                                    },
                                )),
                            };
                            target.command(req, {
                                let latest_config = state.live_configuration.clone();
                                move |resp| {
                                    if let Some(response) = resp.response
                                        && let interface::response::Response::Setconfig(config) =
                                            response
                                    {
                                        *latest_config.lock().unwrap() = config.config;
                                    }
                                }
                            });
                        }
                    }
                    let mut autosave = false;
                    ui.checkbox(&mut autosave, "Autosave");
                    if let Some(live_config) = &*state.live_configuration.lock().unwrap() {
                        if ui.button("Revert").clicked() {
                            *config = live_config.clone();
                        }
                    }
                    if let Some(target) = &state.target {
                        if ui.button("Flash").clicked() {
                            let req = interface::Request {
                                id: 1,
                                request: Some(interface::request::Request::Flashconfig(
                                    interface::request::FlashConfig {},
                                )),
                            };
                            target.command(req, {
                                move |resp| {
                                    println!("Flash command received: {:?}", resp);
                                }
                            });
                        }
                    }
                });

                ui.separator();
                let live_config = state.live_configuration.lock().unwrap();
                let config_ref = if let Some(c) = &*live_config {
                    c
                } else {
                    // We have a desired config but no live config, this should only happen from a
                    // config import with no live target, so to make the UI cleaner lets make the
                    // reference config a copy of our desired config
                    &config.clone()
                };
                config_pane::render_config_pane(ui, config_ref, config);
            });
        }
        egui::TopBottomPanel::bottom("Status").show(ctx, |ui| {
            match &state.target {
                None => ui.label("Target: Not connected"),
                Some(_) => match *state.live_configuration.lock().unwrap() {
                    None => ui.label("Target: Connecting..."),
                    Some(_) => ui.label("Target: Connected"),
                },
            };
            match &state.log {
                None => ui.label("Log: Not connected"),
                Some(log) => {
                    let mut log_str = format!("Log: {}", log.filename());
                    let view = state.view_cache.lock().unwrap();
                    match view.get_status() {
                        view_cache::LoadingStatus::Done => log_str += " Loaded",
                        view_cache::LoadingStatus::Loading { progress } => {
                            should_reload = true;
                            log_str += &format!(" {:.0}%", progress)
                        }
                        view_cache::LoadingStatus::Idle => log_str += " Idle",
                    }
                    let delta = view
                        .get_log_time_range()
                        .and_then(|r| Some(r.max - r.min))
                        .unwrap_or(0);
                    let point_count = view.get_point_count();
                    ui.label(log_str);
                    ui.label(format!(
                        "View: {} points over {} minutes in {} ms",
                        point_count,
                        delta / (60 * 1000 * 1000 * 1000),
                        render_time.as_millis()
                    ))
                }
            };
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            state.logview.ui(ui);
        });
        if should_reload {
            ctx.request_repaint();
        }
    })
}
