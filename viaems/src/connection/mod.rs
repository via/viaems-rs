pub mod usb;
pub mod udp;
pub mod exec;

use std::thread;
use std::time::{Duration, SystemTime};
use std::sync::{atomic, mpsc, Arc};
use crate::interface;

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

pub struct RxMessage {
    pub time: SystemTime,
    pub message: interface::Message,
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


mod stream {
    use std::io::{BufRead, BufReader, Read};
    pub enum Error {
        IOError(std::io::Error),
        FrameDecodeError,
    }

    impl From<std::io::Error> for Error {
        fn from(inner: std::io::Error) -> Error {
            Error::IOError(inner)
        }
    }


    pub struct StreamReader<T> {
        inner: BufReader<T>,
        crc: crc::Crc<u32>,
    }

    impl<T: Read> StreamReader<T> {
        pub fn new(input: T) -> Self {
            Self {
                inner: BufReader::new(input),
                crc: crc::Crc::<u32>::new(&crc::CRC_32_ISO_HDLC),
            }
        }

        pub fn read(&mut self) -> Result<Vec<u8>, Error> {
            let mut raw_bytes = vec![];
            self.inner.read_until(0, &mut raw_bytes)?;

            if raw_bytes.len() < 6 || *raw_bytes.last().unwrap() != 0 {
                return Err(Error::FrameDecodeError);
            }

            let decoded_size = cobs::decode_in_place(raw_bytes.as_mut_slice()).expect("failed to decode cobs frame");
            raw_bytes.truncate(decoded_size);

            let pdu = &raw_bytes[2..raw_bytes.len()-4];

            let len = u16::from_le_bytes(raw_bytes[0..2].try_into().unwrap());
            if len as usize != pdu.len() {
                return Err(Error::FrameDecodeError);
            }

            let crc = u32::from_le_bytes(raw_bytes[decoded_size-4..decoded_size].try_into().unwrap());
            if crc != self.crc.checksum(pdu) {
                return Err(Error::FrameDecodeError);
            }

            Ok(pdu.to_owned())
        }
    }

    pub fn write(pdu: &[u8]) -> Vec<u8> {
        let crc = crc::Crc::<u32>::new(&crc::CRC_32_ISO_HDLC);
        let len_bytes = (pdu.len() as u16).to_le_bytes();
        let crc_bytes = crc.checksum(pdu).to_le_bytes();

        let mut frame = Vec::with_capacity(len_bytes.len() + pdu.len() + crc_bytes.len());
        frame.extend_from_slice(&len_bytes);
        frame.extend_from_slice(pdu);
        frame.extend_from_slice(&crc_bytes);

        let mut cobs = cobs::encode_vec(frame.as_slice());
        cobs.extend_from_slice(&[0 as u8]);

        cobs
    }
}
