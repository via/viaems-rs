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
    update: interface::EngineUpdate,
    update_time: Option<SystemTime>,
}

struct Application {
    target: Option<viaems::Manager>,
    log: Option<viaems::Log>,
    latest_feed: Arc<Mutex<FeedState>>,
    view_cache: Arc<Mutex<view_cache::ViewCache>>,
    logview: log_view::LogViewer,
    file_dialog: FileDialog,

    last_update_time: SystemTime,
    follow_feed: bool,
}

impl<'a> Application {
    fn new() -> Application {
        let feed = Arc::new(Mutex::new(FeedState::default()));
        let dialog = FileDialog::open_file();
        let cache = Arc::new(Mutex::new(view_cache::ViewCache::new()));

        Application {
            target: None,
            log: None,
            latest_feed: feed,
            logview: log_view::LogViewer::new(cache.clone()),
            view_cache: cache,
            file_dialog: dialog,
            last_update_time: SystemTime::now(),
            follow_feed: false,
        }
    }

    fn open_log(&mut self, filename: &str) {
        let log = viaems::Log::new(filename);

        self.view_cache
            .lock().unwrap()
            .set_logreader(log.try_clone().expect("Unable to create logview reader"));
        self.logview.set_available_keys(&log.keys().unwrap_or_default());
        self.log = Some(log);
        println!("Opening log");
    }

    fn connect_udp(&mut self) {
        let devices = connection::udp::detect(connection::udp::DEFAULT_MCAST_ADDR, Some(Duration::from_millis(100)));
        if devices.len() > 0 {
            let conn = connection::Connection::new_udp(&devices[0]);
            let target = viaems::Manager::new(conn);

            let logwriter : Option<viaems::UpdateWriter> = if let Some(reader) = &self.log {
                Some(reader.get_writer().expect("Unable to create log writer"))
            } else {
                None 
            };

            target.on_update({
                let feed_state = self.latest_feed.clone();
                move |time: SystemTime, update: &interface::EngineUpdate| {
                    if let Some(w) = &logwriter {
                        w.add(time, update.clone());
                    }
                    Application::update_feed(&feed_state, time, update);
                }
            });

            self.target = Some(target);
        }
    }

    fn connect_exec(&mut self) {
        let conn = connection::Connection::new_exec("/home/via/dev/viaems/obj/hosted/bleh.sh");
        let target = viaems::Manager::new(conn);
        target.on_update({
            let feed_state = self.latest_feed.clone();
            let view = self.view_cache.clone();
            let logwriter : Option<viaems::UpdateWriter> = if let Some(reader) = &self.log {
                Some(reader.get_writer().expect("Unable to create log writer"))
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

        self.target = Some(target);
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
    }

    eframe::run_simple_native("Viaems UI", options, move |ctx, _frame| {
        ctx.set_pixels_per_point(1.5);

        let now = SystemTime::now();
        let render_time = now.duration_since(state.last_update_time).unwrap();
        let mut should_reload = false;
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
                    if ui.button("Open Sim").clicked() {
                        state.connect_exec();
                        ui.close_menu();
                    }
                });
            });
        });
        if state.target.is_some() {
            should_reload = true;
            egui::SidePanel::left("left panel")
                .min_width(250.0)
                .show(ctx, |ui| {
                ui.label("Live Data");
                let state = state.latest_feed.lock().unwrap();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    egui::CollapsingHeader::new("Sensors").default_open(true).show(ui, |ui| {
                        let sensors = state.update.sensors.unwrap_or_default();
                        let render_fault = |ui: &mut egui::Ui, f: interface::SensorFault| {
                            match f {
                                interface::SensorFault::SensorNoFault =>
                                    ui.label(egui::RichText::new("OK").color(egui::Color32::GREEN)),
                                interface::SensorFault::SensorRangeFault =>
                                    ui.label(egui::RichText::new("BAD RANGE").color(egui::Color32::RED)),
                                interface::SensorFault::SensorConnectionFault =>
                                    ui.label(egui::RichText::new("BAD CONN").color(egui::Color32::RED)),
                            }
                        };

                        egui::Grid::new("sensors")
                            .num_columns(4)
                            .striped(true)
                            .show(ui, |ui| {
                                ui.label("");
                                ui.label("Value");
                                ui.label("Rate");
                                ui.label("Fault");
                                ui.end_row();

                                ui.label("MAP");
                                ui.label(format!("{:.1}", sensors.map));
                                ui.label(format!("{:.1}", sensors.map_rate));
                                render_fault(ui, sensors.map_fault());
                                ui.end_row();

                                ui.label("IAT");
                                ui.label(format!("{:.1}", sensors.iat));
                                ui.label(format!("{:.1}", sensors.iat_rate));
                                render_fault(ui, sensors.iat_fault());
                                ui.end_row();

                                ui.label("CLT");
                                ui.label(format!("{:.1}", sensors.clt));
                                ui.label(format!("{:.1}", sensors.clt_rate));
                                render_fault(ui, sensors.clt_fault());
                                ui.end_row();

                                ui.label("BRV");
                                ui.label(format!("{:.1}", sensors.brv));
                                ui.label(format!("{:.1}", sensors.brv_rate));
                                render_fault(ui, sensors.brv_fault());
                                ui.end_row();

                                ui.label("TPS");
                                ui.label(format!("{:.1}", sensors.tps));
                                ui.label(format!("{:.1}", sensors.tps_rate));
                                render_fault(ui, sensors.tps_fault());
                                ui.end_row();

                                ui.label("AAP");
                                ui.label(format!("{:.1}", sensors.aap));
                                ui.label(format!("{:.1}", sensors.aap_rate));
                                render_fault(ui, sensors.aap_fault());
                                ui.end_row();

                                ui.label("FRT");
                                ui.label(format!("{:.1}", sensors.frt));
                                ui.label(format!("{:.1}", sensors.frt_rate));
                                render_fault(ui, sensors.frt_fault());
                                ui.end_row();

                                ui.label("EGO");
                                ui.label(format!("{:.1}", sensors.ego));
                                ui.label(format!("{:.1}", sensors.ego_rate));
                                render_fault(ui, sensors.ego_fault());
                                ui.end_row();

                                ui.label("FRP");
                                ui.label(format!("{:.1}", sensors.frp));
                                ui.label(format!("{:.1}", sensors.frp_rate));
                                render_fault(ui, sensors.frp_fault());
                                ui.end_row();

                                ui.label("ETH");
                                ui.label(format!("{:.1}", sensors.eth));
                                ui.label(format!("{:.1}", sensors.eth_rate));
                                render_fault(ui, sensors.eth_fault());
                                ui.end_row();

                                ui.label("KNK1");
                                ui.label(sensors.knock1.to_string());
                                ui.end_row();

                                ui.label("KNK2");
                                ui.label(sensors.knock2.to_string());
                                ui.end_row();

                            });

                    });
                    egui::CollapsingHeader::new("Position").default_open(true).show(ui, |ui| {
                        let position = state.update.position.unwrap_or_default();
                        egui::Grid::new("position")
                            .num_columns(2)
                            .striped(true)
                            .show(ui, |ui| {
                                ui.label("Sync");
                                if position.synced {
                                    ui.label(egui::RichText::new("YES").color(egui::Color32::GREEN));
                                } else {
                                    ui.label(egui::RichText::new("NO").color(egui::Color32::RED));
                                }
                                ui.end_row();

                                if !position.synced {
                                    ui.label("Reason");
                                    let reason = position.loss_cause().as_str_name();
                                    let stripped = if let Some(s) = reason.strip_prefix("DECODER_") { s } else { reason };

                                    ui.label(stripped);
                                    ui.end_row();
                                }

                                ui.label("Angle");
                                ui.label(position.last_angle.to_string());
                                ui.end_row();

                                ui.label("RPM");
                                ui.label(position.average_rpm.to_string());
                                ui.end_row();

                            });
                        });
                    egui::CollapsingHeader::new("Calculations").default_open(true).show(ui, |ui| {
                        let calcs = state.update.calculations.unwrap_or_default();
                        egui::Grid::new("calcs")
                            .num_columns(2)
                            .striped(true)
                            .show(ui, |ui| {
                                ui.label("Timing Advance");
                                ui.label(format!("{:.0}", calcs.advance));
                                ui.end_row();

                                ui.label("Dwell (uS)");
                                ui.label(format!("{:.0}", calcs.dwell_us));
                                ui.end_row();

                                ui.label("Fuel (uS)");
                                ui.label(format!("{:.0}", calcs.fuel_us));
                                ui.end_row();

                                ui.label("Lambda");
                                ui.label(format!("{:.3}", calcs.lambda));
                                ui.end_row();

                                ui.label("VE");
                                ui.label(format!("{:.1}", calcs.ve));
                                ui.end_row();

                                ui.label("Temp Enrichment (%)");
                                ui.label(format!("{:.0}", calcs.engine_temp_enrichment));
                                ui.end_row();

                                ui.separator();
                                ui.end_row();

                                ui.label("Airmass per cycle (g)");
                                ui.label(format!("{:.3}", calcs.airmass_per_cycle));
                                ui.end_row();

                                ui.label("Fuel volume per cycle (cc)");
                                ui.label(format!("{:.3}", calcs.fuelvol_per_cycle));
                                ui.end_row();

                                ui.label("Pulsewidth Correction (mS)");
                                ui.label(format!("{:.3}", calcs.pulse_width_correction));
                                ui.end_row();

                                ui.separator();
                                ui.end_row();

                                ui.label("RPM Limiter");
                                if calcs.rpm_limit_cut {
                                    ui.label(egui::RichText::new("ON").color(egui::Color32::RED));
                                } else {
                                    ui.label(egui::RichText::new("OFF").color(egui::Color32::GREEN));
                                }
                                ui.end_row();

                                ui.label("Boost Limiter");
                                if calcs.boost_cut {
                                    ui.label(egui::RichText::new("ON").color(egui::Color32::RED));
                                } else {
                                    ui.label(egui::RichText::new("OFF").color(egui::Color32::GREEN));
                                }
                                ui.end_row();

                                ui.label("Fuel Limiter");
                                if calcs.fuel_overduty_cut {
                                    ui.label(egui::RichText::new("ON").color(egui::Color32::RED));
                                } else {
                                    ui.label(egui::RichText::new("OFF").color(egui::Color32::GREEN));
                                }
                                ui.end_row();

                                ui.label("Dwell Limiter");
                                if calcs.dwell_overduty_cut {
                                    ui.label(egui::RichText::new("ON").color(egui::Color32::RED));
                                } else {
                                    ui.label(egui::RichText::new("OFF").color(egui::Color32::GREEN));
                                }
                                ui.end_row();

                            });
                        });
                    egui::Grid::new("feed")
                        .num_columns(2)
                        .striped(true)
                        .show(ui, |ui| {
                            ui.label("cputime");
                            ui.label(state.update.header.unwrap_or_default().timestamp.to_string());
                            ui.end_row();
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

            ui.input_mut(|i| {
                // TODO this follow mode should be an explicit option in the UI
                if state.follow_feed && state.log.is_some() {
                    let now = SystemTime::now();
                    let twentyago = now - Duration::from_secs(20);
                    state.logview.set_time_range(view_cache::Range::new(
                        twentyago.duration_since(SystemTime::UNIX_EPOCH)
                                  .unwrap()
                                  .as_nanos() as i64,
                        now.duration_since(SystemTime::UNIX_EPOCH)
                                  .unwrap()
                                  .as_nanos() as i64));

                } else if let Some(mut timerange) = state.logview.get_time_range() {
                    // Hack to prevent scroll/drag when settings panes are open
                    if !state.logview.configuring() {
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

                    state.logview.set_time_range(timerange);
                }

                if i.consume_shortcut(&egui::KeyboardShortcut::new(
                        egui::Modifiers::default(),
                        egui::Key::F,
                )) {
                    state.follow_feed = !state.follow_feed;
                }
            });
            ui.with_layout(egui::Layout::top_down(egui::Align::Max),|ui| {
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new("⛶"))
                        .on_hover_text("Show entire log")
                            .clicked() {
                                if let Some(log) = &state.log && 
                                   let Some(start) = log.get_earliest_time() && 
                                   let Some(stop) = log.get_latest_time() {

                                       let range = view_cache::Range::new(
                                           start.duration_since(SystemTime::UNIX_EPOCH)
                                           .unwrap()
                                           .as_nanos() as i64,            
                                           stop.duration_since(SystemTime::UNIX_EPOCH)
                                           .unwrap()
                                           .as_nanos() as i64);

                                       println!("Range: {:?}", range);
                                       state.logview.set_time_range(range);
                                       state.follow_feed = false;
                                }
                            }

                    let follow_button = egui::Button::new("⏭").selected(state.follow_feed);
                    if ui.add(follow_button)
                        .on_hover_text("Update viewer time range for new data automatically")
                            .clicked() {
                                state.follow_feed = !state.follow_feed;
                    }
                });
            });

            state.logview.ui(ui);

        });
        if should_reload {
            ctx.request_repaint();
        }
    })
}
