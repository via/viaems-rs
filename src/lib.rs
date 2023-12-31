pub mod interface;
pub mod connection;
mod log;

pub use log::LogFeedWriter;

use std::thread;
use std::sync::{Mutex, Arc};
use std::time::{Duration, SystemTime};
use std::collections::VecDeque;

type FeedCallback = dyn FnMut(SystemTime, &Vec<String>, &Vec<interface::FeedValue>) -> () + Send; 
type RequestCallback = dyn FnOnce(interface::ResponseValue) -> () + Send; 

struct Command {
  callback: Box<RequestCallback>,
  message: interface::Message,
}

struct ConnectionState {
  on_feed: Option<Box<FeedCallback>>,
  commands: VecDeque<Command>,
  running: bool,
}

pub struct Manager{
  thread: Option<thread::JoinHandle<()>>,
  state: Arc<Mutex<ConnectionState>>,
  writer: connection::Writer,
}

impl Manager {
  pub fn new(connection: Box<dyn connection::Connection + Send>) -> Manager {
    let state = Arc::new(Mutex::new(ConnectionState{
      on_feed: None,
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

    Manager { thread: Some(thread), state, writer }
  }

  fn main_loop(conn: Box<dyn connection::Connection>, state: Arc<Mutex<ConnectionState>>) {
    let mut current_keys : Option<Vec<String>> = None;
    loop {
      match conn.recv(Duration::from_millis(100)) {
        Ok(connection::RxMessage{time, payload}) => {
          match payload {
            interface::Message::Feed{values} => {
              let mut state = state.lock().unwrap();
              if let Some(keys) = &current_keys {
                if let Some(cb) = &mut state.on_feed {
                  cb(time, &keys, &values);
                }
              }
            },
              interface::Message::Description{keys} => {
                current_keys = Some(keys)
              },
              interface::Message::Response { id: _, response } => {
                let mut state = state.lock().unwrap();
                if let Some(command) = state.commands.pop_front() {
                  (command.callback)(response);
                  if let Some(command) = &state.commands.front() {
                    let msg = command.message.clone();
                    conn.get_writer().send(msg);
                  }
                }
              },
              _ => (),
          }
        }

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

  pub fn on_feed<F>(&self, f: F)
  where F: FnMut(SystemTime, &Vec<String>, &Vec<interface::FeedValue>) -> () + Send + 'static {
    let mut locked = self.state.lock().unwrap();
    locked.on_feed = Some(Box::new(f));
  }

  pub fn command<F>(&self, msg: interface::Message, callback: F)
  where F: FnOnce(interface::ResponseValue) -> () + 'static + Send {
    let mut locked = self.state.lock().unwrap();
    if locked.commands.len() == 0 {
      self.writer.send(msg.clone());
    }
    let command = Command { 
            callback: Box::new(callback), 
            message: msg,
    };
    locked.commands.push_back(command);
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
