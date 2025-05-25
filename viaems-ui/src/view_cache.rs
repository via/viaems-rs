use std::collections::HashMap;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime};

use viaems::{self, LogChunk};

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
    cache1000: viaems::LogChunk,
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
                    self.build_decimations(&r);
                    self.reader = Some(r);
                }
            }
        }
    }

    fn clear_decimations(&mut self) {
        let mut state = self.state.lock().unwrap();
        state.cache.clear();
        state.cache10.clear();
        state.cache100.clear();
        state.cache1000.clear();
    }

    fn build_decimations(&mut self, reader: &viaems::LogReader) {
        self.state.lock().unwrap().status = LoadingStatus::Loading { progress: 0.0 };

        let before = SystemTime::now();
        println!("keys()");
        let keys = reader.keys();
        println!("got keys()");
        let refkeys: Vec<&str> = keys.iter().map(|x| x.as_ref()).collect();

        let start = reader
            .get_earliest_time()
            .unwrap_or(SystemTime::now() - Duration::from_secs(20));
        let stop = reader.get_latest_time().unwrap_or(SystemTime::now());
        println!("got times");

        let mut count = 0;

        let mut chunk10 = viaems::LogChunk::new(&refkeys);
        let mut chunk100 = viaems::LogChunk::new(&refkeys);
        let mut chunk1000 = viaems::LogChunk::new(&refkeys);
        println!("starting foreach");
        reader.range_foreach(start, stop, &refkeys, |time, values| -> bool {
            //            chunk.add(time, values);
            count += 1;

            // TODO real decimation algorithm
            if count % 10 == 0 {
                // chunk10.add(time, values);
            }

            if count % 100 == 0 {
                chunk100.add(time, values);
            }

            if count % 1000 == 0 {
                chunk1000.add(time, values);
            }

            if count % 100000 == 0 {
                let start_ns = start
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos() as i64;

                let stop_ns = stop
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos() as i64;
                let percent = 100.0 * (time - start_ns) as f64 / (stop_ns - start_ns) as f64;
                {
                    let mut state = self.state.lock().unwrap();
                    state.cache10 = chunk10.clone();
                    state.cache100 = chunk100.clone();
                    state.cache1000 = chunk1000.clone();
                    state.status = LoadingStatus::Loading {
                        progress: percent as f32,
                    };
                }
            }

            true
        });
        {
            let mut state = self.state.lock().unwrap();
            state.cache10 = chunk10;
            state.cache100 = chunk100;
            state.cache1000 = chunk1000;
            state.status = LoadingStatus::Done;
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
            cache: viaems::LogChunk::default(),
            cache10: viaems::LogChunk::default(),
            cache100: viaems::LogChunk::default(),
            cache1000: viaems::LogChunk::default(),
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

    pub fn with_cache100<F>(&mut self, mut f: F)
    where
        F: FnMut(&viaems::LogChunk),
    {
        let state = self.state.lock().unwrap();
        f(&state.cache100);
    }

    pub fn set_logreader(&mut self, reader: viaems::LogReader) {
        self.cmd_chan
            .send(ViewBackendCommand::Open(reader))
            .unwrap();
    }
}
