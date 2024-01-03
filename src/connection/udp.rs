use std::net::UdpSocket;
use std::thread;
use std::sync::{mpsc, atomic, Arc};
use std::time::{SystemTime, Duration};

use crate::interface;
use crate::connection;

pub struct UdpConnection {
  recv_thr: Option<thread::JoinHandle<()>>,
  write_thr: Option<thread::JoinHandle<()>>,
  running: Arc<atomic::AtomicBool>,
  rx: mpsc::Receiver<connection::RxMessage>,
  tx: mpsc::Sender<interface::Message>,

}

impl UdpConnection {

    pub fn new(local_addr: &str, remote_addr: &str) -> UdpConnection {
        let socket = UdpSocket::bind(local_addr).expect("socket");

        let (recv_tx, recv_rx) = mpsc::channel();
        let (send_tx, send_rx) = mpsc::channel();

        let running = Arc::new(atomic::AtomicBool::new(true));

        UdpConnection {
            recv_thr: Some(thread::spawn({
                let socket = socket.try_clone().unwrap();
                let running = running.clone();
                move || UdpConnection::recv_loop(socket, running, recv_tx)
            })),
            write_thr: Some(thread::spawn({
                let socket = socket;
                let running = running.clone();
                let remote_addr = remote_addr.to_owned();
                move || UdpConnection::send_loop(socket, running, remote_addr, send_rx)
            })),
            running,
            rx: recv_rx,
            tx: send_tx,
        }
    }

    fn send_loop(socket: UdpSocket, running: Arc<atomic::AtomicBool>, addr: String, rx: mpsc::Receiver<interface::Message>) {
        loop {
            if !running.load(atomic::Ordering::Relaxed) {
              break;
            }

            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(msg) => {
                    let bytes = serde_cbor::to_vec(&msg).unwrap();
                    socket.send_to(&bytes[..], &addr).unwrap();
                },
                Err(mpsc::RecvTimeoutError::Timeout) => (),
                _ => break,
            }
        }
    }

    fn recv_loop(socket: UdpSocket, running: Arc<atomic::AtomicBool>, tx: mpsc::Sender<connection::RxMessage>) {
        socket.set_read_timeout(Some(Duration::from_millis(100))).unwrap();
        let mut recvbuf = [0; 16384];
        loop {
          if !running.load(atomic::Ordering::Relaxed) {
            break;
          }

          let recvd = socket.recv_from(&mut recvbuf);
          match recvd {
            Ok((n_bytes, _)) => {
              let n = serde_cbor::de::from_slice(&recvbuf[0..n_bytes]).unwrap();
              if tx.send(connection::RxMessage{
                  time: SystemTime::now(),
                  payload: n,
              }).is_err() { break; }
            },
            Err(e) => match e.kind() {
              std::io::ErrorKind::TimedOut => (),
              std::io::ErrorKind::WouldBlock => (),
              x => println!("{}, {}", e, x),
            },
        }
      }
    }
}

impl From<mpsc::RecvTimeoutError> for connection::ConnError {
    fn from(inner: mpsc::RecvTimeoutError) -> connection::ConnError {
        match inner {
            mpsc::RecvTimeoutError::Timeout => connection::ConnError::Timeout,
            _ => connection::ConnError::Disconnected,
        }
    }
}

impl connection::Connection for UdpConnection {

    fn recv(&self, timeout: Duration) -> Result<connection::RxMessage, connection::ConnError> {
      Ok(self.rx.recv_timeout(timeout)?)
    }

    fn get_writer(&self) -> connection::Writer {
      return connection::Writer { 
          tx: self.tx.clone(),
      }
    }
}

impl Drop for UdpConnection {
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
