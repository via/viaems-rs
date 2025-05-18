use std::collections::HashMap;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime};

use viaems;

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
            width: 0,
        }
    }
}

#[derive(Clone)]
pub struct ViewData {
    pub config: ViewportConfig,
    pub data: HashMap<String, Vec<f32>>,
}

struct ViewSharedState {
    status: LoadingStatus,

    cache: viaems::LogChunk,
    cache10: viaems::LogChunk,
    cache100: viaems::LogChunk,
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
    last_status_update: SystemTime,
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
                    self.build_decimations(&r);
                    self.reader = Some(r);
                }
            }
        }
    }

    fn update_status(&mut self, status: LoadingStatus) {
        let now = SystemTime::now();
        if now.duration_since(self.last_status_update).unwrap() > Duration::from_millis(10) {
            self.state.lock().unwrap().status = status;
            self.last_status_update = now;
        }
    }

    fn clear_decimations(&mut self) {
        let mut state = self.state.lock().unwrap();
        state.cache.clear();
        state.cache10.clear();
        state.cache100.clear();
    }

    fn build_decimations(&mut self, reader: &viaems::LogReader) {
        self.state.lock().unwrap().status = LoadingStatus::Loading { progress: 0.0 };

        let keys = reader.keys();
        let refkeys: Vec<&str> = keys.iter().map(|x| x.as_ref()).collect();

        let start = reader
            .get_earliest_time()
            .unwrap_or(SystemTime::now() - Duration::from_secs(20));
        let stop = reader.get_latest_time().unwrap_or(SystemTime::now());

        let total_count = reader.get_range_count(start, stop);
        let mut count = 0;
        let mut chunk = viaems::LogChunk::new(&refkeys);
        let before = SystemTime::now();
        reader.range_foreach(start, stop, &refkeys, |time, values| -> bool {
            chunk.add(time, values);

            count += 1;

            //            let percent = 100.0 * count as f32 / total_count as f32;
            self.update_status(LoadingStatus::Loading { progress: 10.0 });

            true
        });
        let after = SystemTime::now();
        println!(
            "Read {} in {} ms",
            chunk.times.len(),
            (after.duration_since(before).unwrap().as_millis())
        );

        self.state.lock().unwrap().cache = chunk;

        self.state.lock().unwrap().status = LoadingStatus::Done;
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
            cache: viaems::LogChunk::default(),
            cache10: viaems::LogChunk::default(),
            cache100: viaems::LogChunk::default(),
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
                    last_status_update: SystemTime::now(),
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
    pub fn render(&mut self) -> () {}

    pub fn set_logreader(&mut self, reader: viaems::LogReader) {
        self.cmd_chan
            .send(ViewBackendCommand::Open(reader))
            .unwrap();
    }
}
