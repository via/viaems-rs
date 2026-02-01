use std::sync::{mpsc, atomic, Arc};
use crate::connection::{Connection, RxMessage};
use crate::interface;
use std::io::{BufRead, BufReader, Write};

use std::process::{Command, Stdio};
use std::time::{SystemTime, Duration};

impl Connection {
    pub fn new_exec(binary: &str) -> Connection {
        let mut subproc = Command::new(binary)
            .stdout(Stdio::piped())
            .stdin(Stdio::piped())
            .spawn()
            .expect("Failed to start binary");

        let running = Arc::new(atomic::AtomicBool::new(true));
        let (recv_tx, recv_rx) = mpsc::channel();

        let rx_thr = std::thread::spawn({
            let stdout = subproc.stdout.take().unwrap();
            let mut buffered_stdout = BufReader::new(stdout);
            let running = running.clone();
            move || {
                let mut raw_bytes = vec![];
                let CRC32 = crc::Crc::<u32>::new(&crc::CRC_32_ISO_HDLC);
                loop {
                    if !running.load(atomic::Ordering::Relaxed) {
                        break;
                    }

                    raw_bytes.clear();
                    buffered_stdout.read_until(0, &mut raw_bytes).expect("failed to read bytes");

                    // Len + CRC is 6 byte minimum
                    if raw_bytes.len() < 6 || *raw_bytes.last().unwrap() != 0 {
                        break;
                    }

                    let decoded_size = cobs::decode_in_place(raw_bytes.as_mut_slice()).expect("failed to decode cobs frame");
                    raw_bytes.truncate(decoded_size);

                    let pdu = &raw_bytes[2..raw_bytes.len()-4];

                    let len = u16::from_le_bytes(raw_bytes[0..2].try_into().unwrap());
                    if len as usize != pdu.len() {
                        println!("Frame is invalid length!");
                        continue;
                    }
                    let crc = u32::from_le_bytes(raw_bytes[decoded_size-4..decoded_size].try_into().unwrap());
                    if crc != CRC32.checksum(pdu) {
                        println!("Invalid CRC");
                        continue;
                    }
                    match prost::Message::decode(pdu) {
                        Ok(message) => {
                            let time = SystemTime::now();
                            if recv_tx.send(RxMessage{time, message}).is_err() { break; }
                        },
                        Err(e) => {
                            println!("Failed to decode! {e}");
                        },
                    }
                }
            }});

                
        let (send_tx, send_rx) = mpsc::channel::<interface::Message>();

        let tx_thr = std::thread::spawn({
            let mut stdin = subproc.stdin.take().unwrap();
            let running = running.clone();
            move || {
                let CRC32 = crc::Crc::<u32>::new(&crc::CRC_32_ISO_HDLC);
                loop {
                    if !running.load(atomic::Ordering::Relaxed) {
                        break;
                    }
                    match send_rx.recv_timeout(Duration::from_millis(100)) {
                        Ok(command) => {
                            let pdu = prost::Message::encode_to_vec(&command);
                            let len_bytes = (pdu.len() as u16).to_le_bytes();
                            let crc_bytes = CRC32.checksum(pdu.as_slice()).to_le_bytes();

                            let mut frame = Vec::with_capacity(len_bytes.len() + pdu.len() + crc_bytes.len());
                            frame.extend_from_slice(&len_bytes);
                            frame.extend_from_slice(pdu.as_slice());
                            frame.extend_from_slice(&crc_bytes);

                            let mut cobs = cobs::encode_vec(frame.as_slice());
                            cobs.extend_from_slice(&[0 as u8]);

                            stdin.write(cobs.as_slice()).unwrap();
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => continue,
                        _ => break,
                    }
                }
            }});


        Connection {
            recv_thr: Some(rx_thr),
            write_thr: Some(tx_thr),
            running,
            rx: recv_rx,
            tx: send_tx
        }


    }
}
