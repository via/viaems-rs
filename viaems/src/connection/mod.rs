pub mod usb;
pub mod udp;
pub mod exec;

use std::thread;
use std::time::{Duration, SystemTime};
use std::sync::{atomic, mpsc, Arc};
use crate::interface;

pub struct RxMessage {
    pub time: SystemTime,
    pub payload: interface::Message,
}

pub enum ConnError {
    Timeout,
    Disconnected,
}

impl From<mpsc::RecvTimeoutError> for ConnError {
    fn from(inner: mpsc::RecvTimeoutError) -> ConnError {
        match inner {
            mpsc::RecvTimeoutError::Timeout => ConnError::Timeout,
            _ => ConnError::Disconnected,
        }
    }
}


pub struct Writer {
    tx: mpsc::Sender<interface::Message>,
}

impl Writer {
    pub fn send(&self, msg: interface::Message) {
        self.tx.send(msg).unwrap();
    }
}

pub struct Connection {
  recv_thr: Option<thread::JoinHandle<()>>,
  write_thr: Option<thread::JoinHandle<()>>,
  running: Arc<atomic::AtomicBool>,
  rx: mpsc::Receiver<RxMessage>,
  tx: mpsc::Sender<interface::Message>,
}

impl Drop for Connection {
  fn drop(&mut self) {
    self.running.store(false, atomic::Ordering::Relaxed);
      if let Some(t) = self.recv_thr.take() {
          t.join().unwrap();
      }
      if let Some(t) = self.write_thr.take() {
          t.join().unwrap();
      }
  }
}



impl Connection {
    pub fn recv(&self, timeout: Duration) -> Result<RxMessage, ConnError> {
      return Ok(self.rx.recv_timeout(timeout)?);
    }

    pub fn get_writer(&self) -> Writer {
        Writer { tx: self.tx.clone() }
    }
}
