use viaems::{self, connection, interface};

use clap::{Parser, Subcommand, ValueEnum};
use ctrlc;
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime};

#[derive(Parser, Debug)]
struct CliArgs {
    #[command(subcommand)]
    command: CliCommands,

    #[arg(short = 'c', value_enum, default_value_t = ConnectionMode::Usb)]
    mode: ConnectionMode,

    #[arg(short = 'd', long)]
    udpdest: Option<String>,
}

#[derive(ValueEnum, Clone, Debug)]
enum ConnectionMode {
    Usb,
    Udp,
}

#[derive(Subcommand, Debug)]
enum CliCommands {
    Record {
        #[arg(default_value = "log.duckdb")]
        filename: String,
    },
    Bootloader,
    Read {
        filename: String,
    },
}

fn main() {
    let args = CliArgs::parse();

    let connection: Box<dyn connection::Connection + Send> = match args.mode {
        ConnectionMode::Udp => {
            let dest = if let Some(s) = args.udpdest {
                s.parse().expect("failed to parse udp destination")
            } else {
                connection::DEFAULT_MCAST_ADDR
            };

            let devices = connection::UdpConnection::detect(dest, Some(Duration::from_millis(100)));
            if devices.len() == 0 {
                panic!("Unable to detect ViaEMS and not dest provided");
            }

            println!("Connecting to {:?} via {:?}", devices[0].target_ucast_ipaddr, devices[0].local_ipaddr);
            Box::new(connection::UdpConnection::new(&devices[0]))
        },
        ConnectionMode::Usb => Box::new(connection::UsbConnection::new()),
    };
    

    match args.command {
        CliCommands::Record { filename } => {

            let manager = viaems::Manager::new(connection);
            record(&filename, manager)
        }
        CliCommands::Bootloader => {
            let manager = viaems::Manager::new(connection);
            bootloader(manager)
        }
        CliCommands::Read { filename } => read(&filename),
    }
}

fn read(filename: &str) {
    let reader = viaems::LogReader::new(filename);
    let mut count = 0;
    reader
        .query_arrow(
            SystemTime::UNIX_EPOCH,
            SystemTime::now(),
            &["rpm", "sensor.map"],
            |batch| {
                println!("batch with {} rows", batch.num_rows());
                count += 1;
            },
        )
        .unwrap();
    println!("Read {} rows", count);
}

fn bootloader(manager: viaems::Manager) {
    manager.blocking_command(viaems::interface::Message::Request(
        viaems::interface::RequestMessage::Bootloader,
    ));
}

enum StatusMsg {
    Terminate,
    FeedCount { count: u64, rate: f64 },
}

fn record(filename: &str, manager: viaems::Manager) {
    let (status_chan_tx, status_chan) = mpsc::channel::<StatusMsg>();

    manager.on_feed({
        let status_chan_tx = status_chan_tx.clone();
        let mut writer: Option<viaems::LogFeedWriter> = None;
        let filename = filename.to_owned();
        let mut total_count = 0;
        let mut this_count = 0;
        let mut time_of_last_msg = Instant::now();
        move |time: SystemTime, keys: &Vec<String>, vals: &Vec<interface::FeedValue>| {
            if writer.is_none() {
                writer = Some(
                    viaems::LogFeedWriter::new(&filename, keys.clone(), vals.clone()).unwrap(),
                );
            }
            if let Some(w) = &mut writer {
                w.add(time, vals.clone());
            }
            this_count += 1;
            let duration = Instant::now() - time_of_last_msg;
            if duration >= Duration::from_secs(1) {
                total_count += this_count;
                status_chan_tx
                    .send(StatusMsg::FeedCount {
                        count: total_count,
                        rate: this_count as f64 / duration.as_secs_f64(),
                    })
                    .unwrap();
                this_count = 0;
                time_of_last_msg += duration;
            }
        }
    });

    ctrlc::set_handler(move || status_chan_tx.send(StatusMsg::Terminate).unwrap()).unwrap();

    loop {
        match status_chan.recv_timeout(Duration::from_millis(1200)) {
            Ok(StatusMsg::Terminate) => break,
            Ok(StatusMsg::FeedCount { count, rate }) => {
                println!("Connected! {} feed points received ({:.0}/s)", count, rate);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                println!("No new data");
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}
