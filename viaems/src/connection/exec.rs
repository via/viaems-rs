use std::sync::{mpsc, atomic, Arc};
use crate::connection::{Connection, RxMessage};
use crate::interface;
use std::io::{BufRead, BufReader};

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
                loop {
                    if !running.load(atomic::Ordering::Relaxed) {
                        break;
                    }

                    raw_bytes.clear();
                    buffered_stdout.read_until(0, &mut raw_bytes).expect("failed to read bytes");
                    let decoded_size = cobs::decode_in_place(raw_bytes.as_mut_slice()).expect("failed to decode cobs frame");
                    raw_bytes.truncate(decoded_size);
                    let pdu = &raw_bytes[2..raw_bytes.len()-4];
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
                loop {
                    if !running.load(atomic::Ordering::Relaxed) {
                        break;
                    }
                    match send_rx.recv_timeout(Duration::from_millis(100)) {
                        Ok(_) => {
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
