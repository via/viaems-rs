use std::collections::HashMap;
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
    Idle,
}

#[derive(Clone, Copy)]
pub struct Range<T> {
    pub min: T,
    pub max: T,
}

impl<T: PartialOrd + Copy> Range<T> {
    pub fn new(min: T, max: T) -> Range<T> {
        Range { min, max }
    }

    pub fn expand_value(&mut self, value: T) {
        if value > self.max {
            self.max = value;
        }

        if value < self.min {
            self.min = value;
        }
    }

    pub fn expand_range(&mut self, r: Self) {
        if r.max > self.max {
            self.max = r.max;
        }

        if r.min < self.min {
            self.min = r.min;
        }
    }
}

/// Stores a single data point that represents a summary of a time range
#[derive(Clone, Copy)]
pub struct PointSummary {
    /// Time range represented by the point
    pub time: Range<i64>,

    /// Value range represented by the point, but potentially not loaded yet
    pub value: Range<f32>,
}

impl PointSummary {
    fn expand_to_include_value(&mut self, time: i64, value: f32) {
        self.time.expand_value(time);
        self.value.expand_value(value);
    }

    fn expand_to_include_summary(&mut self, summary: &Self) {
        self.time.expand_range(summary.time);
        self.value.expand_range(summary.value);
    }

    fn new(time: i64, value: f32) -> PointSummary {
        PointSummary {
            time: Range::<i64>::new(time, time),
            value: Range::<f32>::new(value, value),
        }
    }
}

/// Actual state that is shared between the frontend ViewCache and the Backend worker
struct ViewSharedState {
    status: LoadingStatus,

    /// After loading, contains the point count for display purposes
    point_count: usize,

    /// After loading, contains time range for the log
    time_range: Option<Range<i64>>,

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
    SetHotCache { range: Range<i64> },
}

struct SummaryBuilder {
    count: usize,
    summary: Option<PointSummary>,
}

impl SummaryBuilder {
    fn has_time_gap(&self, time: i64) -> bool {
        if let Some(summary) = &self.summary {
            time - summary.time.max > Duration::from_secs(1).as_nanos() as i64
        } else {
            false
        }
    }

    fn add(&mut self, time: i64, value: f32) {
        if let Some(summary) = &mut self.summary {
            summary.expand_to_include_value(time, value);
        } else {
            self.summary = Some(PointSummary::new(time, value));
        }
        self.count += 1;
    }

    fn reset(&mut self) {
        self.summary = None;
        self.count = 0;
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
                    let earliest = r.get_earliest_time().unwrap();
                    let latest = r.get_latest_time().unwrap();
                    let total_count = r.point_count_in_range(earliest, latest).unwrap();

                    let earliest_ns = earliest
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap()
                        .as_nanos() as i64;
                    let latest_ns = latest
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap()
                        .as_nanos() as i64;

                    self.state.lock().unwrap().point_count = total_count as usize;
                    self.state.lock().unwrap().time_range =
                        Some(Range::new(earliest_ns, latest_ns));

                    self.set_status(LoadingStatus::Done);
                    self.reader = Some(r);
                }
                Ok(ViewBackendCommand::SetHotCache { range: _ }) => {}
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
            summary: None,
        });

        current_cache10000.resize_with(refkeys.len(), || SummaryBuilder {
            count: 0,
            summary: None,
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
                        if pg100.count == 100 || pg100.has_time_gap(times[row_idx]) {
                            // Complete the group, add to the result
                            let mut locked = self.state.lock().unwrap();
                            locked
                                .cache100
                                .entry(col_name.to_owned())
                                .or_insert(vec![])
                                .push(pg100.summary.clone().unwrap());
                            pg100.reset();
                        }

                        pg100.add(times[row_idx], value);

                        if pg10000.count == 10000 || pg10000.has_time_gap(times[row_idx]) {
                            // Complete the group, add to the result
                            let mut locked = self.state.lock().unwrap();
                            locked
                                .cache10000
                                .entry(col_name.to_owned())
                                .or_insert(vec![])
                                .push(pg10000.summary.clone().unwrap());
                            pg10000.reset();
                        }
                        pg10000.add(times[row_idx], value);
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
            time_range: None,
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

    pub fn render(
        &mut self,
        times: Range<i64>,
        key: &str,
        width: usize,
    ) -> Vec<Option<PointSummary>> {
        // Render what data is immediately available (from decimation cache) into a
        // Vec of PointSummaryS of length `width` for `key`. If the provided range and
        // width demands more resolution than the cache provides, a hot store of
        // un-summarized points will be used -- and if it is not applicable, an update
        // request will be sent to the backend
        //

        let ns_per_pixel = (times.max - times.min) / width as i64;
        let mut render = Vec::<Option<PointSummary>>::new();
        render.resize_with(width, || None);

        let locked = self.state.lock().unwrap();

        let cache = if ns_per_pixel > 5000000000 {
            // More than 5 seconds per pixel
            &locked.cache10000
        } else if ns_per_pixel > 50000000 {
            // more than 50 ms per pixel
            &locked.cache100
        } else {
            &locked.cache100
            // Does the hotcache contain what we need?
        };

        let cache = match cache.get(key) {
            Some(x) => x,
            None => return render,
        };

        let cache_start_idx = cache.partition_point(|x| x.time.min < times.min);
        let cache_end_idx = cache.partition_point(|x| x.time.max < times.max);
        for idx in cache_start_idx..cache_end_idx {
            let start_pos = ((cache[idx].time.min - times.min) / ns_per_pixel) as usize;
            let end_pos = ((cache[idx].time.max - times.min) / ns_per_pixel) as usize;
            if end_pos >= width {
                println!(
                    "width: {} start_pos: {} end_pos: {}",
                    width, start_pos, end_pos
                );
            }
            assert!(start_pos <= end_pos);
            assert!(end_pos < width);

            for pos in start_pos..=end_pos {
                match &mut render[pos as usize] {
                    None => render[pos as usize] = Some(cache[idx]),
                    Some(x) => {
                        x.expand_to_include_summary(&cache[idx]);
                    }
                }
            }
        }

        render
    }

    pub fn get_status(&self) -> LoadingStatus {
        self.state.lock().unwrap().status.clone()
    }

    pub fn get_point_count(&self) -> usize {
        self.state.lock().unwrap().point_count
    }

    pub fn get_log_time_range(&self) -> Option<Range<i64>> {
        self.state.lock().unwrap().time_range.clone()
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
