use std::sync::{mpsc, atomic, Arc};
use std::time::{SystemTime, Duration};
use crate::interface;
use crate::connection::{Connection, ConnError, RxMessage, Writer};
use rusb::{Context, UsbContext, HotplugBuilder, Device};
use rusb_async::TransferPool;

pub struct UsbConnection {
    recv_rx: mpsc::Receiver<RxMessage>,
    send_tx: mpsc::Sender<interface::Message>,
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
        let mut pool = TransferPool::new(devh.clone()).expect("could not create pool");

        let (recv_tx, recv_rx) = mpsc::channel();

        std::thread::spawn({
            move || {
                for _ in 1..=4 {
                    let mut buf : Vec<u8> = vec![];
                    buf.reserve(16384);
                    pool.submit_bulk(0x82, buf).unwrap();
                }
                loop {
                    match pool.poll(Duration::from_secs(1)) {
                        Ok(bytes) => {
                            let payload = serde_cbor::de::from_slice(&bytes[..]).unwrap();
                            let time = SystemTime::now();

                            if recv_tx.send(RxMessage{time, payload}).is_err() { break; }
                            pool.submit_bulk(0x82, bytes).unwrap();
                        },
                        Err(e) => {println!("{e:?}"); break},
                    }

                }
            }
        });

        let mut anotherpool = TransferPool::new(devh).expect("could not create pool");
        let (send_tx, send_rx) = mpsc::channel::<interface::Message>();

        std::thread::spawn({
            move || {
                loop {
                    match send_rx.recv() {
                        Ok(msg) => {
                            let bytes = serde_cbor::to_vec(&msg).unwrap();
                            anotherpool.submit_bulk(0x01, bytes).unwrap();
                        }
                        _ => break,
                    }
                }
            }
        });


        UsbConnection { recv_rx, send_tx }
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
