use std::sync::{mpsc, atomic, Arc};
use std::time::{SystemTime, Duration};
use crate::interface;
use rusb::{Context, UsbContext, HotplugBuilder, Device};
use rusb_async::TransferPool;

pub struct UsbConnection {
    rx: mpsc::Receiver<(i64, interface::Message)>,
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
        let mut pool = TransferPool::new(Arc::new(devh)).expect("could not create pool");
        let (tx, rx) = mpsc::channel();
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
                            let unix_time : i64 =
                                SystemTime::now()
                                .duration_since(SystemTime::UNIX_EPOCH).unwrap()
                                .as_nanos().try_into().unwrap();
                            let n = serde_cbor::de::from_slice(&bytes[..]).unwrap();
                            if tx.send((unix_time, n)).is_err() { break; }
                            pool.submit_bulk(0x82, bytes).unwrap();
                        },
                        Err(e) => {println!("{e:?}"); break},
                    }

                }
            }
        });


        UsbConnection { rx }
    }
    pub fn recv(&self, timeout: Duration) -> Result<(i64, interface::Message), mpsc::RecvTimeoutError> {
      return self.rx.recv_timeout(timeout);
    }
}
