#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use clap::Parser;
use egui_file::FileDialog;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};
use viaems::{self, connection, interface};

mod log_view;
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

    view: Rc<RefCell<view_cache::ViewCache>>,
    logview: log_view::LogViewer,
    file_dialog: FileDialog,

    last_update_time: SystemTime,
}

impl<'a> Application {
    fn new() -> Application {
        let feed = Arc::new(Mutex::new(FeedState::default()));
        let cwd = std::env::current_dir().ok();
        let dialog = FileDialog::open_file(cwd);
        let cache = Rc::new(RefCell::new(view_cache::ViewCache::new()));

        Application {
            target: None,
            log: None,
            latest_feed: feed,
            logview: log_view::LogViewer::new(cache.clone()),
            view: cache,
            file_dialog: dialog,
            last_update_time: SystemTime::now(),
        }
    }

    fn open_log(&mut self, filename: &str) {
        self.log = Some(viaems::LogReader::new(filename));

        self.view
            .borrow_mut()
            .set_logreader(viaems::LogReader::new(filename));
        println!("Opening log");
    }

    fn connect_udp(&mut self) {
        let devices = connection::udp::detect(connection::DEFAULT_MCAST_ADDR, Some(Duration::from_millis(100)));
        if devices.len() > 0 {
            let conn = connection::Connection::new_udp(&devices[0]);
            let target = viaems::Manager::new(conn);
            target.on_feed({
                let feed_state = self.latest_feed.clone();
                move |time: SystemTime, keys: &Vec<String>, values: &Vec<interface::FeedValue>| {
                    Application::update_feed(&feed_state, time, keys, values)
                }
            });

            self.target = Some(target);
        }
    }

    fn connect_usb(&mut self) {
        let conn = Box::new(connection::UsbConnection::new());
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
        ctx.set_pixels_per_point(1.5);

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
                    if ui.button("Open USB").clicked() {
                        state.connect_usb();
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
                    let viewstat = state.view.borrow().get_status();
                    match viewstat {
                        view_cache::LoadingStatus::Done => log_str += " Loaded",
                        view_cache::LoadingStatus::Loading { progress } => {
                            log_str += &format!(" {:.0}%", progress)
                        }
                        view_cache::LoadingStatus::Idle => log_str += " Idle",
                    }
                    let delta = state
                        .view
                        .borrow()
                        .get_log_time_range()
                        .and_then(|r| Some(r.max - r.min))
                        .unwrap_or(0);
                    let point_count = state.view.borrow().get_point_count();
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
            ui.input(|i| {
                if let Some(mut timerange) = state.logview.get_time_range() {
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

                    state.logview.set_time_range(timerange);
                }
            });
            state.logview.ui(ui);
        });
        ctx.request_repaint();
    })
}
