use std::sync::{mpsc, atomic, Arc};
use std::time::{SystemTime, Duration};
use crate::interface;
use crate::connection::{Connection, ConnError, RxMessage, Writer};
use rusb::{Context, UsbContext, HotplugBuilder, Device};
use rusb_async::TransferPool;

pub struct UsbConnection {
    recv_rx: mpsc::Receiver<RxMessage>,
    send_tx: mpsc::Sender<interface::Message>,
    running: Arc<atomic::AtomicBool>,
    recv_thread: Option<std::thread::JoinHandle<()>>,
    send_thread: Option<std::thread::JoinHandle<()>>,
}

struct UsbHandle {}

impl<T: UsbContext> rusb::Hotplug<T> for UsbHandle {
    fn device_arrived(&mut self, device: Device<T>) {
        println!("ARRIVED {:?}", device);
    }
    fn device_left(&mut self, device: Device<T>) {
        println!("LEFT {:?}", device);
    }
}

impl UsbConnection {
    pub fn new() -> UsbConnection {
//            let bleh = HotplugBuilder::new()
//                .vendor_id(0x0483)
//                .product_id(0x5740)
//                .enumerate(true)
//                .register::<Context, _>(&context, Box::new(UsbHandle{})).unwrap();
//
//            move || {
//                let bleh = bleh;
//                loop {
//                    context.handle_events(None).unwrap();
//                }
//            }});
        let context = Context::new().unwrap();
        let mut devh = context.open_device_with_vid_pid(0x0483, 0x5740).expect("Could not open device");
        for i in 0..=2 {
            if devh.kernel_driver_active(i).unwrap() {
                devh.detach_kernel_driver(i).expect("Could not detach kernel from device");
            }
        }

        let devh = Arc::new(devh);
        let running = Arc::new(atomic::AtomicBool::new(true));

        let (recv_tx, recv_rx) = mpsc::channel();
        let recv_thread = std::thread::spawn({
            let mut pool = TransferPool::new(devh.clone()).expect("could not create pool");
            let running = running.clone();
            move || {
                for _ in 1..=4 {
                    let mut buf : Vec<u8> = vec![];
                    buf.reserve(16384);
                    pool.submit_bulk(0x82, buf).unwrap();
                }
                loop {
                    if !running.load(atomic::Ordering::Relaxed) {
                      break;
                    }
                    match pool.poll(Duration::from_secs(1)) {
                        Ok(bytes) => {
                            match serde_cbor::de::from_slice(&bytes[..]) {
                              Ok(payload) => {
                                let time = SystemTime::now();
                                if recv_tx.send(RxMessage{time, payload}).is_err() { break; }
                                pool.submit_bulk(0x82, bytes).unwrap();
                              },
                              Err(e) => println!("Failed to decode! {e}"),
                            }
                        },
                        Err(e) => {
                          println!("{e:?}"); 
                        },
                    }

                }
            }
        });

        let (send_tx, send_rx) = mpsc::channel::<interface::Message>();
        let send_thread = std::thread::spawn({
            let running = running.clone();
            move || {
                loop {
                    if !running.load(atomic::Ordering::Relaxed) {
                      break;
                    }
                    match send_rx.recv_timeout(Duration::from_millis(100)) {
                        Ok(msg) => {
                            let bytes = serde_cbor::to_vec(&msg).unwrap();
                            devh.write_bulk(0x01, &bytes[..], Duration::from_secs(1)).unwrap();
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => continue,
                        _ => break,
                    }
                }
            }
        });


        UsbConnection { 
          recv_rx, 
          send_tx, 
          running, 
          recv_thread: Some(recv_thread),
          send_thread: Some(send_thread),
        }
    }
}

impl Drop for UsbConnection {
  fn drop(&mut self) {
    self.running.store(false, atomic::Ordering::Relaxed);
      if let Some(t) = self.recv_thread.take() {
          t.join().unwrap();
      }
      if let Some(t) = self.send_thread.take() {
          t.join().unwrap();
      }
  }
}



impl Connection for UsbConnection {
    fn recv(&self, timeout: Duration) -> Result<RxMessage, ConnError> {
      return Ok(self.recv_rx.recv_timeout(timeout)?);
    }

    fn get_writer(&self) -> Writer {
        Writer { tx: self.send_tx.clone() }
    }
}
