use std::net::UdpSocket;
use std::thread;
use std::sync::{mpsc, atomic, Arc};
use std::time::{SystemTime, Duration};
use std::net::{Ipv4Addr, SocketAddrV4};

use network_interface::{NetworkInterface, NetworkInterfaceConfig, Addr};

use crate::interface;
use crate::connection::{Connection, RxMessage};

#[derive(Debug)]
pub struct UdpDevice {
    pub local_ipaddr: Ipv4Addr,
    pub target_ucast_ipaddr: SocketAddrV4,
    pub target_mcast_ipaddr: SocketAddrV4,

}

pub const DEFAULT_MCAST_ADDR : SocketAddrV4 = SocketAddrV4::new(Ipv4Addr::new(239, 0, 0, 10), 5556);

pub fn detect(mcast_dest: SocketAddrV4, timeout: Option<Duration>) -> Vec<UdpDevice> {
    let interfaces = NetworkInterface::show().expect("unable to enumerate interfaces");

    let mut results = Vec::new();
    let mut local_addrs = Vec::new();
    for interface in interfaces {
        for addr in interface.addr {
            if let Addr::V4(v4) = addr {
                local_addrs.push(v4.ip);
            }
        }
    }

    for laddr in &local_addrs {
        let socket = UdpSocket::bind(("0.0.0.0", mcast_dest.port())).expect("socket bind failed");
        socket.join_multicast_v4(&mcast_dest.ip(), laddr).expect("unable to join mcast group");

        socket.set_read_timeout(timeout).expect("Unable to set socket timeout");
        let mut rcvbuf = [0 as u8; 1500];
        if let Ok((_, source)) = socket.recv_from(&mut rcvbuf) {

            let result = UdpDevice {
                target_ucast_ipaddr: if let std::net::SocketAddr::V4(v4) = source { v4 } else { unreachable!() },
                target_mcast_ipaddr: mcast_dest,
                local_ipaddr: laddr.clone(),
            };
            results.push(result);
        }
    }
    results
}


impl Connection {
    pub fn new_udp(device: &UdpDevice) -> Connection {
        let socket = UdpSocket::bind((device.local_ipaddr, device.target_mcast_ipaddr.port())).expect("socket");
        socket.join_multicast_v4(&device.target_mcast_ipaddr.ip(), &device.local_ipaddr).unwrap();

        let (recv_tx, recv_rx) = mpsc::channel();
        let (send_tx, send_rx) = mpsc::channel();

        let running = Arc::new(atomic::AtomicBool::new(true));

        Connection {
            recv_thr: Some(thread::spawn({
                let socket = socket.try_clone().unwrap();
                let running = running.clone();
                move || recv_loop(socket, running, recv_tx)
            })),
            write_thr: Some(thread::spawn({
                let socket = socket;
                let running = running.clone();
                let remote_addr = device.target_ucast_ipaddr;
                move || send_loop(socket, running, remote_addr, send_rx)
            })),
            running,
            rx: recv_rx,
            tx: send_tx,
        }
    }
}

fn send_loop(socket: UdpSocket, running: Arc<atomic::AtomicBool>, addr: SocketAddrV4, rx: mpsc::Receiver<interface::Message>) {
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

fn recv_loop(socket: UdpSocket, running: Arc<atomic::AtomicBool>, tx: mpsc::Sender<RxMessage>) {
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
          if tx.send(RxMessage{
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
