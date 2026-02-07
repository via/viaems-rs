use std::io::Write;
use std::sync::{Arc, atomic, mpsc};
use std::time::{Duration, SystemTime};

use nusb::MaybeFuture;
use nusb::transfer::{Bulk, ControlOut, In, Out};

use crate::connection::{Connection, RxMessage, stream};
use crate::interface;

impl Connection {
    pub fn new_usb() -> Connection {
        const VIAEMS_VID: u16 = 0x1209;
        const VIAEMS_PID: u16 = 0x2041;

        const USB_IN_EP: u8 = 0x81;
        const USB_OUT_EP: u8 = 0x01;

        let deviceinfo = nusb::list_devices()
            .wait()
            .unwrap()
            .find(|d| d.vendor_id() == VIAEMS_VID && d.product_id() == VIAEMS_PID)
            .expect("Unable to find device");

        let device = deviceinfo.open().wait().unwrap();
        let interface = device.detach_and_claim_interface(1).wait().unwrap();

        // ViaEMS TinyUSB needs DTR to make the transmit buffer non-overwritable
        interface
            .control_out(
                ControlOut {
                    control_type: nusb::transfer::ControlType::Class,
                    recipient: nusb::transfer::Recipient::Interface,
                    request: 0x22,
                    value: 3, // DTR | RTS
                    index: 0,
                    data: &[],
                },
                Duration::from_millis(100),
            )
            .wait()
            .unwrap();

        let rx = interface
            .endpoint::<Bulk, In>(USB_IN_EP)
            .unwrap()
            .reader(1024)
            .with_num_transfers(4)
            .with_read_timeout(Duration::from_millis(100));

        let mut tx = interface
            .endpoint::<Bulk, Out>(USB_OUT_EP)
            .unwrap()
            .writer(1024)
            .with_num_transfers(4)
            .with_write_timeout(Duration::from_millis(100));

        let running = Arc::new(atomic::AtomicBool::new(true));

        let (recv_tx, recv_rx) = mpsc::channel();
        let recv_thread = std::thread::spawn({
            let running = running.clone();
            let mut stream_reader = stream::StreamReader::new(rx);
            move || {
                loop {
                    if !running.load(atomic::Ordering::Relaxed) {
                        break;
                    }
                    match stream_reader.read() {
                        Err(stream::Error::IOError(x)) => {
                            println!("Failed to read from target: {}", x);
                            break;
                        }
                        Err(stream::Error::FrameDecodeError) => continue,
                        Ok(pdu) => {
                            match prost::Message::decode(pdu.as_slice()) {
                                Ok(message) => {
                                    let time = SystemTime::now();
                                    if recv_tx.send(RxMessage { time, message }).is_err() {
                                        break;
                                    }
                                }
                                Err(e) => {
                                    println!("Failed to decode! {e}");
                                    continue;
                                }
                            };
                        }
                    };
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
                        Ok(command) => {
                            let pdu = prost::Message::encode_to_vec(&command);
                            let encoded = stream::write(pdu.as_slice());
                            let mut position = 0;

                            while position < encoded.len() {
                                let written = tx.write(&encoded.as_slice()[position..]).unwrap();
                                tx.flush_end().unwrap();
                                println!("write {written}");
                                position += written;
                            }
                            println!("Done");
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
