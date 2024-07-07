#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use eframe::egui;
use viaems::{self, interface, connection};
use std::sync::{Arc, Mutex, atomic};
use std::time::SystemTime;

#[derive(Default)]
struct FeedState {
    keys: Vec<String>,
    values: Vec<interface::FeedValue>,
}

fn main() -> Result<(), eframe::Error> {

//    let conn = Box::new(connection::UdpConnection::new("127.0.0.1:5556", "127.0.0.1:5555"));
    let conn = Box::new(connection::UsbConnection::new());
    let manager = viaems::Manager::new(conn);

    let options = eframe::NativeOptions::default();

    let count = Arc::new(atomic::AtomicUsize::new(0));

    let feed_state = Arc::new(Mutex::new(FeedState::default()));

    manager.on_feed({
        let count = count.clone();
        let feed_state = feed_state.clone();
        move |_: SystemTime, keys: &Vec<String>, values: &Vec<interface::FeedValue>| {
            let mut state = feed_state.lock().unwrap();
            if state.keys.len() != keys.len() {
                state.keys = keys.clone();
            }
            state.values = values.clone();
            count.fetch_add(1, atomic::Ordering::Relaxed);
        }});

    // Our application state:
    let mut name = "Arthur".to_owned();
    let mut age = 42;

    eframe::run_simple_native("My egui App", options, move |ctx, _frame| {
        egui::SidePanel::left("left panel").show(ctx, |ui| {
          ui.label("Live Data");
          let state = feed_state.lock().unwrap();
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
        egui::TopBottomPanel::bottom("bottom").show(ctx, |ui| {
            ui.label("Bottom");
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("test");
            ui.horizontal(|ui| {
                let name_label = ui.label("Your name: ");
                ui.text_edit_singleline(&mut name)
                    .labelled_by(name_label.id);
            });
            ui.add(egui::Slider::new(&mut age, 0..=120).text("age"));
            if ui.button("Click each year").clicked() {
                age += 1;
            }
            ui.label(format!("Hello '{name}', age {age}"));
            let current_count = count.load(atomic::Ordering::Relaxed);
            ui.label(format!("Current count: {current_count}"));
        });
        ctx.request_repaint();
    })
}
