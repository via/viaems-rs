pub mod connection;
pub mod interface;
mod log;

pub use log::UpdateWriter;
pub use log::LogReader;

use std::collections::VecDeque;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime};

pub use duckdb::arrow;

type UpdateCallback = dyn FnMut(SystemTime, &interface::EngineUpdate) -> () + Send;
type RequestCallback = dyn FnOnce(interface::Response) -> () + Send;

struct Command {
    callback: Box<RequestCallback>,
    request: interface::Request,
}

struct ConnectionState {
    on_update: Option<Box<UpdateCallback>>,
    commands: VecDeque<Command>,
    running: bool,
}

pub struct Manager {
    thread: Option<thread::JoinHandle<()>>,
    state: Arc<Mutex<ConnectionState>>,
    writer: connection::Writer,
}

impl Manager {
    pub fn new(connection: connection::Connection) -> Manager {
        let state = Arc::new(Mutex::new(ConnectionState {
            on_update: None,
            commands: VecDeque::new(),
            running: true,
        }));

        let writer = connection.get_writer();
        let thread = thread::spawn({
            let state = state.clone();
            || {
                Self::main_loop(connection, state);
            }
        });

        Manager {
            thread: Some(thread),
            state,
            writer,
        }
    }

    fn main_loop(conn: connection::Connection, state: Arc<Mutex<ConnectionState>>) {
        loop {
            match conn.recv(Duration::from_millis(100)) {
                Ok(connection::RxMessage { time, message }) => match message.msg {
                    Some(interface::message::Msg::EngineUpdate(eu)) => {
                        let mut state = state.lock().unwrap();
                        if let Some(cb) = &mut state.on_update {
                            cb(time, &eu);
                        }
                    },
                    Some(interface::message::Msg::Response(response)) => {
                        let mut state = state.lock().unwrap();
                        if let Some(command) = state.commands.pop_front() {
                            (command.callback)(response);
                            if let Some(command) = &state.commands.front() {
                                let req = command.request.clone();
                                conn.get_writer().send(interface::Message{msg: Some(interface::message::Msg::Request(req))});
                            }
                        }
                    },
                    _ => (),
                },

                Err(connection::ConnError::Timeout) => (),
                _ => break,
            }
            // Exit condition
            let state = state.lock().unwrap();
            if !state.running {
                break;
            }
        }
    }

    pub fn on_update<F>(&self, f: F)
    where
        F: FnMut(SystemTime,  &interface::EngineUpdate) -> () + Send + 'static,
    {
        let mut locked = self.state.lock().unwrap();
        locked.on_update = Some(Box::new(f));
    }

    pub fn command<F>(&self, req: interface::Request, callback: F)
    where
        F: FnOnce(interface::Response) -> () + 'static + Send,
    {
        let mut locked = self.state.lock().unwrap();
        if locked.commands.len() == 0 {
            self.writer.send(interface::Message{ msg: Some(interface::message::Msg::Request(req.clone()))});
        }
        let command = Command {
            callback: Box::new(callback),
            request: req,
        };
        locked.commands.push_back(command);
    }

    pub fn blocking_request(&self, req: interface::Request) -> interface::Response {
        let (tx, rx) = mpsc::channel::<interface::Response>();

        self.command(req, move |resp: interface::Response| {
            tx.send(resp).unwrap();
        });

        rx.recv().unwrap()
    }
}

impl Drop for Manager {
    fn drop(&mut self) {
        {
            let mut state = self.state.lock().unwrap();
            state.running = false;
        }
        self.thread.take().unwrap().join().unwrap();
    }
}
