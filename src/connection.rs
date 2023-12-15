use std::thread;
use std::sync::{mpsc, atomic, Arc};
use std::net::UdpSocket;
use std::time::{SystemTime, Duration};

use crate::interface;

pub struct UdpConnection {
  socket: UdpSocket,
  remote_addr: String,
  thr: thread::JoinHandle<()>,
  running: Arc<atomic::AtomicBool>,
  rx: mpsc::Receiver<(i64, interface::Message)>,

}

pub struct UdpConnectionWriter {
  socket: UdpSocket,
  remote_addr: String,
}

impl UdpConnection {
    pub fn new(local_addr: &str, remote_addr: &str) -> UdpConnection {
        let socket = UdpSocket::bind(local_addr).expect("socket");
        let (tx, rx) = mpsc::channel();
        let running = Arc::new(atomic::AtomicBool::new(true));
        let thr = thread::spawn({
          let socket = socket.try_clone().unwrap();
          let running = running.clone();
          socket.set_read_timeout(Some(Duration::from_millis(100))).unwrap();
          move || {
            let mut recvbuf = [0; 16384];
            loop {
              if !running.load(atomic::Ordering::Relaxed) {
                break;
              }

              let recvd = socket.recv_from(&mut recvbuf);
              match recvd {
                Ok((n_bytes, _)) => {
                  let unix_time : i64 =
                  SystemTime::now()
                      .duration_since(SystemTime::UNIX_EPOCH).unwrap()
                      .as_nanos().try_into().unwrap();
                  let n = serde_cbor::de::from_slice(&recvbuf[0..n_bytes]).unwrap();
                  if tx.send((unix_time, n)).is_err() { break; }
                },
                Err(e) => match e.kind() {
                  std::io::ErrorKind::TimedOut => (),
                  std::io::ErrorKind::WouldBlock => (),
                  x => println!("{}, {}", e, x),
                },
            }
          }
        }});

        UdpConnection { 
          socket, 
          remote_addr: remote_addr.to_string(), 
          thr, 
          rx ,
          running,
        }
    }

    pub fn recv(&self, timeout: Duration) -> Result<(i64, interface::Message), mpsc::RecvTimeoutError> {
      return self.rx.recv_timeout(timeout);
    }

    pub fn get_writer(&self) -> UdpConnectionWriter {
      return UdpConnectionWriter { 
        socket: self.socket.try_clone().unwrap(),
        remote_addr: self.remote_addr.clone(),
      }
    }

    pub fn close(self) {
      self.running.store(false, atomic::Ordering::Relaxed);
      self.thr.join().unwrap();
    }
}

impl UdpConnectionWriter {
  pub fn send(&self, m: &interface::Message) {
    let bytes = serde_cbor::to_vec(m).unwrap();
    self.socket.send_to(&bytes[..], &self.remote_addr).unwrap();
  }
}
