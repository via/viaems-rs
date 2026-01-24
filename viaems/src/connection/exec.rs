use std::sync::{mpsc, atomic, Arc};
use crate::connection::{Connection, RxMessage};
use crate::interface;

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
            let running = running.clone();
            move || {
                let mut deser = serde_cbor::Deserializer::from_reader(stdout);
                loop {
                    if !running.load(atomic::Ordering::Relaxed) {
                        break;
                    }
                    match serde::de::Deserialize::deserialize(&mut deser) {
                        Ok(payload) => {
                            let time = SystemTime::now();
                            if recv_tx.send(RxMessage{time, payload}).is_err() { break; }
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
                        Ok(msg) => {
                            serde_cbor::to_writer(&mut stdin, &msg).expect("Could not write to process");
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
