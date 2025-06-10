use std::collections::HashMap;
use std::ops::Range;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime};

use egui::TextBuffer;
use viaems;
use viaems::arrow::array::{AsArray, Datum};
use viaems::arrow::compute;

#[derive(Clone)]
pub enum LoadingStatus {
    Done,
    Loading { progress: f32 },
    Idle,
}

/// Stores a single data point that represents a summary of a time range
#[derive(Clone)]
pub struct PointSummary {
    /// Time range represented by the point
    pub time: Range<i64>,

    /// Value range represented by the point, but potentially not loaded yet
    pub value: Range<f32>,
}

/// Actual state that is shared between the frontend ViewCache and the Backend worker
struct ViewSharedState {
    status: LoadingStatus,

    /// After loading, contains the point count for display purposes
    point_count: usize,

    // Store un-summarized view of a single time range
    hotcache: HashMap<String, Vec<PointSummary>>,

    // Store un-summarized view of a single time range
    new_data: HashMap<String, Vec<PointSummary>>,

    // Store summarized view of entire log
    cache100: HashMap<String, Vec<PointSummary>>,
    cache10000: HashMap<String, Vec<PointSummary>>,
}

pub struct ViewCache {
    state: Arc<Mutex<ViewSharedState>>,
    cmd_chan: mpsc::Sender<ViewBackendCommand>,
    thread: thread::JoinHandle<()>,
    keys: Vec<String>,
}

enum ViewBackendCommand {
    Close,
    Open(viaems::LogReader),
    Die,
    SetKeys { keys: Vec<String> },
    SetHotCache { start_ns: i64, stop_ns: i64 },
}

struct SummaryBuilder {
    count: usize,
    summary: PointSummary,
}

impl SummaryBuilder {
    fn has_time_gap(&self, time: i64) -> bool {
        time - self.summary.time.end > Duration::from_secs(1).as_nanos() as i64
    }

    fn add(&mut self, time: i64, value: f32) {
        self.summary.time.end = time;
        if value > self.summary.value.end {
            self.summary.value.end = value;
        }
        if value < self.summary.value.start {
            self.summary.value.start = value;
        }
        self.count += 1;
    }
}

struct Backend {
    state: Arc<Mutex<ViewSharedState>>,
    cmd_chan: mpsc::Receiver<ViewBackendCommand>,
    reader: Option<viaems::LogReader>,
}

impl Backend {
    fn backend_command_loop(&mut self) {
        loop {
            match self.cmd_chan.recv() {
                Err(_) => return,
                Ok(ViewBackendCommand::Die) => return,
                Ok(ViewBackendCommand::Close) => {
                    self.reader = None;
                    self.set_status(LoadingStatus::Idle);
                }
                Ok(ViewBackendCommand::Open(r)) => {
                    // Determine overall point count of the file
                    self.set_status(LoadingStatus::Loading { progress: 0.0 });
                    let earliest = SystemTime::UNIX_EPOCH;
                    let latest = SystemTime::now();
                    let total_count = r.point_count_in_range(earliest, latest).unwrap();
                    self.state.lock().unwrap().point_count = total_count as usize;
                    self.set_status(LoadingStatus::Done);
                    self.reader = Some(r);
                }
                Ok(ViewBackendCommand::SetHotCache { start_ns, stop_ns }) => {}
                Ok(ViewBackendCommand::SetKeys { keys }) => {
                    self.update_decimations(keys);
                }
            }
        }
    }

    fn set_status(&self, status: LoadingStatus) {
        self.state.lock().unwrap().status = status;
    }

    fn update_decimations(&mut self, new_keys: Vec<String>) {
        self.set_status(LoadingStatus::Loading { progress: 0.0 });

        let before = SystemTime::now();
        let reader = if let Some(r) = &self.reader {
            r
        } else {
            return;
        };

        let start = SystemTime::UNIX_EPOCH;
        let stop = SystemTime::now(); // TODO this should be "max" time

        let mut current_count = 0;

        let refkeys: Vec<&str> = new_keys.iter().map(|x| x.as_str()).collect();

        let mut current_cache100 = Vec::new();
        let mut current_cache10000 = Vec::new();

        current_cache100.resize_with(refkeys.len(), || SummaryBuilder {
            count: 0,
            summary: PointSummary {
                time: 0..0,
                value: 0.0..0.0,
            },
        });

        current_cache10000.resize_with(refkeys.len(), || SummaryBuilder {
            count: 0,
            summary: PointSummary {
                time: 0..0,
                value: 0.0..0.0,
            },
        });

        reader
            .query_arrow(start, stop, &refkeys, |batch| {
                let times = batch
                    .column(0)
                    .as_primitive::<viaems::arrow::datatypes::Int64Type>()
                    .values();

                current_count += batch.num_rows();
                for (idx, data) in batch.columns().iter().skip(1).enumerate() {
                    let col_name = refkeys[idx];

                    let casted;
                    let values = match data.data_type() {
                        viaems::arrow::datatypes::DataType::UInt32 => {
                            casted =
                                compute::cast(data, &viaems::arrow::datatypes::DataType::Float32)
                                    .unwrap();
                            casted
                                .as_primitive::<viaems::arrow::datatypes::Float32Type>()
                                .values()
                        }
                        viaems::arrow::datatypes::DataType::Int64 => {
                            casted =
                                compute::cast(data, &viaems::arrow::datatypes::DataType::Float32)
                                    .unwrap();
                            casted
                                .as_primitive::<viaems::arrow::datatypes::Float32Type>()
                                .values()
                        }
                        viaems::arrow::datatypes::DataType::Float32 => data
                            .as_primitive::<viaems::arrow::datatypes::Float32Type>()
                            .values(),
                        viaems::arrow::datatypes::DataType::Float64 => {
                            casted =
                                compute::cast(data, &viaems::arrow::datatypes::DataType::Float32)
                                    .unwrap();
                            casted
                                .as_primitive::<viaems::arrow::datatypes::Float32Type>()
                                .values()
                        }
                        _ => panic!("Unrecognized type in conversion: {}", data.data_type()),
                    };

                    let pg100 = &mut current_cache100[idx];
                    let pg10000 = &mut current_cache10000[idx];

                    for row_idx in 0..batch.num_rows() {
                        let value = values[row_idx];

                        // If its the first one, reset everything
                        if pg100.count == 0 {
                            pg100.summary.time = times[row_idx]..times[row_idx];
                            pg100.summary.value = value..value;
                        }

                        if pg10000.count == 0 {
                            pg10000.summary.time = times[row_idx]..times[row_idx];
                            pg10000.summary.value = value..value;
                        }

                        pg100.add(times[row_idx], value);
                        pg10000.add(times[row_idx], value);

                        if pg100.count == 100 {
                            // Complete the group, add to the result
                            pg100.count = 0;
                            let mut locked = self.state.lock().unwrap();
                            locked
                                .cache100
                                .entry(col_name.to_owned())
                                .or_insert(vec![])
                                .push(pg100.summary.clone());
                        }

                        if pg10000.count == 10000 {
                            // Complete the group, add to the result
                            pg10000.count = 0;
                            let mut locked = self.state.lock().unwrap();
                            locked
                                .cache10000
                                .entry(col_name.to_owned())
                                .or_insert(vec![])
                                .push(pg10000.summary.clone());
                        }
                    }
                }

                {
                    let mut locked = self.state.lock().unwrap();
                    let progress = current_count as f32 / locked.point_count as f32 * 100.0;
                    locked.status = LoadingStatus::Loading { progress };
                }
            })
            .unwrap();

        self.set_status(LoadingStatus::Done);
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

        let shared_state = Arc::new(Mutex::new(ViewSharedState {
            status: LoadingStatus::Done,
            point_count: 0,
            hotcache: HashMap::default(),
            new_data: HashMap::default(),
            cache100: HashMap::default(),
            cache10000: HashMap::default(),
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
                    backend.backend_command_loop();
                }
            })
            .unwrap();
        ViewCache {
            state: shared_state,
            cmd_chan: cmd_chan_tx,
            thread: backend_thread,
            keys: vec![],
        }
    }

    pub fn render(&mut self, times: Range<i64>, key: &str, width: usize) {
        // Render what data is immediately available (from decimation cache) into a
        // Vec of PointSummaryS of length `width` for `key`. If the provided range and
        // width demands more resolution than the cache provides, a hot store of
        // un-summarized points will be used -- and if it is not applicable, an update
        // request will be sent to the backend
        //

        let ns_per_pixel = (times.end - times.start) / width as i64;

        if ns_per_pixel > 5000000000 { // More than 5 seconds per pixel
             // Use cache10k
        } else if ns_per_pixel > 50000000 { // more than 50 ms per pixel
             // Use cache 100
        } else {
            // Does the hotcache contain what we need?
        }
    }

    pub fn get_status(&self) -> LoadingStatus {
        self.state.lock().unwrap().status.clone()
    }

    pub fn get_point_count(&self) -> usize {
        self.state.lock().unwrap().point_count
    }

    pub fn with_cache100<F>(&self, mut f: F)
    where
        F: FnMut(&HashMap<String, Vec<PointSummary>>),
    {
        let state = self.state.lock().unwrap();
        f(&state.cache100);
    }

    pub fn with_cache10000<F>(&self, mut f: F)
    where
        F: FnMut(&HashMap<String, Vec<PointSummary>>),
    {
        let state = self.state.lock().unwrap();
        f(&state.cache10000);
    }

    pub fn set_logreader(&mut self, reader: viaems::LogReader) {
        self.cmd_chan
            .send(ViewBackendCommand::Open(reader))
            .unwrap();
        self.cmd_chan
            .send(ViewBackendCommand::SetKeys {
                keys: vec![
                    "rpm".to_owned(),
                    "sensor.map".to_owned(),
                    "sensor.ego".to_owned(),
                ],
            })
            .unwrap();
    }
}
