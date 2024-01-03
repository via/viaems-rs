mod usb;
mod udp;

use std::time::{Duration, SystemTime};
use std::sync::mpsc;
use crate::interface;

pub use usb::UsbConnection;
pub use udp::UdpConnection;

pub struct RxMessage {
    pub time: SystemTime,
    pub payload: interface::Message,
}

pub enum ConnError {
    Timeout,
    Disconnected,
}

pub struct Writer {
    tx: mpsc::Sender<interface::Message>,
}

impl Writer {
    pub fn send(&self, msg: interface::Message) {
        self.tx.send(msg).unwrap();
    }
}

pub trait Connection {
    fn recv(&self, timeout: Duration) -> Result<RxMessage, ConnError>;
    fn get_writer(&self) -> Writer;
}
