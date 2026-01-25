use std::sync::{mpsc, atomic, Arc};
use std::time::{SystemTime, Duration};
use crate::interface;
use crate::connection::{Connection, ConnError, RxMessage, Writer};
use rusb::{Context, UsbContext, Device};
use rusb_async::TransferPool;


impl Connection {
    pub fn new_usb() -> Connection {
        let context = Context::new().unwrap();
        let devh = context.open_device_with_vid_pid(0x1209, 0x2041).expect("Could not open device");
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
                    pool.submit_bulk(0x81, buf).unwrap();
                }
                loop {
                    if !running.load(atomic::Ordering::Relaxed) {
                      break;
                    }
                    match pool.poll(Duration::from_secs(1)) {
                        Ok(bytes) => {
//                            match serde_cbor::de::from_slice(&bytes[..]) {
//                              Ok(payload) => {
//                                let time = SystemTime::now();
//                                if recv_tx.send(RxMessage{time, payload}).is_err() { break; }
//                                pool.submit_bulk(0x81, bytes).unwrap();
//                              },
//                              Err(e) => {
//                                  println!("Failed to decode! {e}");
//                                  pool.submit_bulk(0x81, bytes).unwrap();
//                              }
//                            }
                        },
                        Err(e) => {
                          println!("Failed to poll: {e:?}"); 
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
//                            let bytes = serde_cbor::to_vec(&msg).unwrap();
//                            devh.write_bulk(0x01, &bytes[..], Duration::from_secs(1)).unwrap();
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => continue,
                        _ => break,
                    }
                }
            }
        });


        Connection { 
          recv_thr: Some(recv_thread),
          write_thr: Some(send_thread),
          running, 
          rx: recv_rx, 
          tx: send_tx, 
        }
    }
}

