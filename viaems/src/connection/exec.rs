use std::sync::{mpsc, atomic, Arc};
use crate::connection::{stream, Connection, RxMessage};
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
            let mut stream_reader = stream::StreamReader::new(stdout);
            let running = running.clone();
            move || {
                loop {
                    if !running.load(atomic::Ordering::Relaxed) {
                        break;
                    }

                    match stream_reader.read() {
                        Err(stream::Error::IOError(x)) => {
                            println!("Failed to read from target: {}", x);
                            break;
                        },
                        Err(stream::Error::FrameDecodeError) => continue,
                        Ok(pdu) => {
                            match prost::Message::decode(pdu.as_slice()) {
                                Ok(message) => {
                                    let time = SystemTime::now();
                                    if recv_tx.send(RxMessage{time, message}).is_err() { break; }
                                },
                                Err(e) => {
                                    println!("Failed to decode! {e}");
                                    continue;
                                },
                            };
                        }
                    };

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
                        Ok(command) => {
                            let pdu = prost::Message::encode_to_vec(&command);
                            let encoded = stream::write(pdu.as_slice()); 
                            let mut position = 0;

                            while position < encoded.len() {
                                position += stdin.write(&encoded.as_slice()[position..]).unwrap();
                            }
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
