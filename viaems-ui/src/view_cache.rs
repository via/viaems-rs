use std::collections::HashMap;
use std::ops::Deref;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime};

use viaems;
use viaems::arrow::array::AsArray;
use viaems::arrow::compute;

#[derive(Clone)]
pub enum LoadingStatus {
    Done,
    Loading { progress: f32 },
}

#[derive(Clone)]
pub struct ViewportConfig {
    pub start: SystemTime,
    pub stop: SystemTime,
    pub width: u32,
}

impl Default for ViewportConfig {
    fn default() -> ViewportConfig {
        ViewportConfig {
            start: SystemTime::now() - Duration::from_secs(20),
            stop: SystemTime::now(),
            width: 10000,
        }
    }
}

#[derive(Clone)]
pub struct ViewPixel {
    pub exists: bool,
    pub min: f32,
    pub max: f32,
}

#[derive(Clone)]
pub struct ViewData {
    pub config: ViewportConfig,
    pub data: HashMap<String, Vec<ViewPixel>>,
}

struct ViewSharedState {
    status: LoadingStatus,
    data: ViewData,
}

pub struct ViewCache {
    state: Arc<Mutex<ViewSharedState>>,
    cmd_chan: mpsc::Sender<ViewBackendCommand>,
    thread: thread::JoinHandle<()>,
}

enum ViewBackendCommand {
    Close,
    Open(viaems::LogReader),
    Die,
}

struct Backend {
    state: Arc<Mutex<ViewSharedState>>,
    cmd_chan: mpsc::Receiver<ViewBackendCommand>,
    reader: Option<viaems::LogReader>,
}

impl Backend {
    fn backend_render_loop(&mut self) {
        loop {
            match self.cmd_chan.recv() {
                Err(_) => return,
                Ok(ViewBackendCommand::Die) => return,
                Ok(ViewBackendCommand::Close) => {
                    self.reader = None;
                    self.clear_decimations();
                }
                Ok(ViewBackendCommand::Open(r)) => {
                    self.build_viewport(&r);
                    self.reader = Some(r);
                }
            }
        }
    }

    fn clear_decimations(&mut self) {
        let mut state = self.state.lock().unwrap();
        state.data.data.clear();
    }

    fn build_viewport(&mut self, reader: &viaems::LogReader) {
        self.state.lock().unwrap().status = LoadingStatus::Loading { progress: 0.0 };

        let before = SystemTime::now();
        //      let keys = reader.keys();
        let keys = vec!["rpm", "sensor.map", "sensor.ego"];
        let refkeys: Vec<&str> = keys.iter().map(|x| x.as_ref()).collect();

        let start = reader
            .get_earliest_time()
            .unwrap_or(SystemTime::now() - Duration::from_secs(20));
        let stop = reader.get_latest_time().unwrap_or(SystemTime::now());
        let start_ns = start
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as i64;

        let stop_ns = stop
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as i64;
        println!("got times");

        let total_count = reader.point_count_in_range(start, stop).unwrap();

        let mut count = 0;
        let mut batch_count = 0;

        let config = self.state.lock().unwrap().data.config.clone();
        let mut cache: HashMap<String, Vec<ViewPixel>> = HashMap::new();
        for k in &keys {
            cache.insert(
                k.to_string(),
                vec![
                    ViewPixel {
                        exists: false,
                        min: f32::MAX,
                        max: f32::MIN
                    };
                    config.width as usize
                ],
            );
        }

        reader
            .query_arrow(start, stop, &refkeys, |batch| {
                let times = batch
                    .column(0)
                    .as_primitive::<viaems::arrow::datatypes::Int64Type>();
                let pixel_indexes_array =
                    times.unary::<_, viaems::arrow::datatypes::Int64Type>(|time| {
                        let pixel_idx = (((time - start_ns) as f64 / (stop_ns - start_ns) as f64)
                            * config.width as f64) as i64;
                        pixel_idx
                    });
                let pixel_indexes = pixel_indexes_array.values();

                for (idx, data) in batch.columns().iter().skip(1).enumerate() {
                    let col_name = keys[idx];
                    let pixels = cache.get_mut(col_name).unwrap();

                    let casted;
                    let values = match data.data_type() {
                        viaems::arrow::datatypes::DataType::Float32 => data
                            .as_primitive::<viaems::arrow::datatypes::Float32Type>()
                            .values(),
                        viaems::arrow::datatypes::DataType::UInt32 => {
                            casted =
                                compute::cast(data, &viaems::arrow::datatypes::DataType::Float32)
                                    .unwrap();
                            casted
                                .as_primitive::<viaems::arrow::datatypes::Float32Type>()
                                .values()
                        }
                        _ => panic!("Unrecognized type in conversion"),
                    };
                    for row_idx in 0..batch.num_rows() {
                        let pixel_idx = pixel_indexes[row_idx] as usize;
                        let value = values[row_idx];

                        if value < pixels[pixel_idx].min {
                            pixels[pixel_idx].min = value;
                        }
                        if value > pixels[pixel_idx].max {
                            pixels[pixel_idx].max = value;
                        }
                        pixels[pixel_idx].exists = true;
                    }
                }

                count += batch.num_rows();
                batch_count += 1;

                if batch_count % 100 == 0 {
                    {
                        let mut state = self.state.lock().unwrap();
                        let progress = count as f32 / total_count as f32 * 100.0;
                        state.status = LoadingStatus::Loading { progress };
                        state.data.data = cache.clone();
                    }
                }
            })
            .unwrap();
        {
            let mut state = self.state.lock().unwrap();
            state.status = LoadingStatus::Done;
            state.data.data = cache;
        }
        let after = SystemTime::now();
        println!(
            "build_decimations took {} ms",
            after.duration_since(before).unwrap().as_millis()
        );
    }
}

impl ViewCache {
    pub fn new() -> ViewCache {
        // Default to a 20 second empty view
        let view = ViewData {
            config: ViewportConfig::default(),
            data: HashMap::default(),
        };

        let shared_state = Arc::new(Mutex::new(ViewSharedState {
            status: LoadingStatus::Done,
            data: view,
        }));
        let (cmd_chan_tx, cmd_chan_rx) = mpsc::channel::<ViewBackendCommand>();
        let backend_thread = thread::Builder::new()
            .name("render-backend".to_string())
            .spawn({
                let shared_state = shared_state.clone();
                let mut backend = Backend {
                    state: shared_state.clone(),
                    cmd_chan: cmd_chan_rx,
                    reader: None,
                };
                move || {
                    backend.backend_render_loop();
                }
            })
            .unwrap();
        ViewCache {
            state: shared_state,
            cmd_chan: cmd_chan_tx,
            thread: backend_thread,
        }
    }

    pub fn get_status(&self) -> LoadingStatus {
        self.state.lock().unwrap().status.clone()
    }

    pub fn with_viewport<F>(&self, mut f: F)
    where
        F: FnMut(&ViewData),
    {
        let state = self.state.lock().unwrap();
        f(&state.data);
    }

    pub fn set_logreader(&mut self, reader: viaems::LogReader) {
        self.cmd_chan
            .send(ViewBackendCommand::Open(reader))
            .unwrap();
    }
}
