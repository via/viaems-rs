use viaems::{self, connection, interface, analyze};

use clap::{Parser, Subcommand, ValueEnum};
use ctrlc;
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime};

#[derive(Parser, Debug)]
struct CliArgs {
    #[command(subcommand)]
    command: CliCommands,

    #[arg(short = 'c', value_enum, default_value_t = ConnectionMode::NoConnection)]
    mode: ConnectionMode,

    #[arg(short = 'd', long)]
    udpdest: Option<String>,

    #[arg(short = 'e', long)]
    exec: Option<String>,
}

#[derive(ValueEnum, Clone, Debug)]
enum ConnectionMode {
    NoConnection,
    Usb,
    Udp,
    Exec,
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
    Analyze {
        filename: String,
    },
}

fn main() {
    let args = CliArgs::parse();

    let connection = match args.mode {
        ConnectionMode::Exec => {
            let binary = args.exec.unwrap_or("viaems".into());
            Some(connection::Connection::new_exec(&binary))
        }
        ConnectionMode::Udp => {
            let dest = if let Some(s) = args.udpdest {
                s.parse().expect("failed to parse udp destination")
            } else {
                connection::udp::DEFAULT_MCAST_ADDR
            };

            let devices = connection::udp::detect(dest, Some(Duration::from_millis(100)));
            if devices.len() == 0 {
                panic!("Unable to detect ViaEMS and not dest provided");
            }

            println!(
                "Connecting to {:?} via {:?}",
                devices[0].target_ucast_ipaddr, devices[0].local_ipaddr
            );
            Some(connection::Connection::new_udp(&devices[0]))
        }
        ConnectionMode::Usb => Some(connection::Connection::new_usb()),
        _ => None,
    };

    match args.command {
        CliCommands::Record { filename } => {
            let conn = connection.expect("Record mode requires a connection!");
            let manager = viaems::Manager::new(conn);
            record(&filename, manager)
        }
        CliCommands::Bootloader => {
            let conn = connection.expect("Bootloader mode requires a connection!");
            let manager = viaems::Manager::new(conn);
            bootloader(manager)
        }
        CliCommands::Read { filename } => read(&filename),
        CliCommands::Analyze { filename } =>  {
            let reader = viaems::Log::new(&filename);
            let results = viaems::analyze::get_correction_points(&reader);
            let rpms : &[f32] = &[0.0, 500.0, 1000.0, 1500.0, 2000.0, 2500.0, 3000.0, 3500.0, 4000.0, 4500.0, 5000.0, 5500.0, 6000.0, 6500.0, 7000.0];

            let maps : &[f32] = &[5.0, 10.0, 15.0, 20.0, 25.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0, 100.0, 120.0, 150.0, 200.0, 250.0, 300.0];

            print!("     ");
            for ri in 1..(rpms.len()-1) {
                print!("{:>4.0}  ", rpms[ri]);
            }
            println!();
            for map_idx in 1..(maps.len() - 1) {
                print!("{:>3.0}  ", maps[map_idx]); 
                for rpm_idx in 1..(rpms.len() - 1) {
                    let rs = (rpms[rpm_idx - 1], rpms[rpm_idx], rpms[rpm_idx + 1]);
                    let ms = (maps[map_idx - 1], maps[map_idx], maps[map_idx + 1]);

                    let est = viaems::analyze::estimate_point(rs, ms, &results);
                    print!("{:>4.0}  ", est.ve);
                }
                println!();
            }
        },
    }
}

fn read(filename: &str) {
    let reader = viaems::Log::new(filename);
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
    manager.blocking_request(viaems::interface::Request {
        id: 0,
        request: Some(viaems::interface::request::Request::Resettobootloader(
            viaems::interface::request::ResetToBootloader {},
        )),
    });
}

enum StatusMsg {
    Terminate,
    UpdateCount { count: u64, rate: f64 },
}

fn record(filename: &str, manager: viaems::Manager) {
    let (status_chan_tx, status_chan) = mpsc::channel::<StatusMsg>();
    let log = viaems::Log::new(filename);
    let writer = log.get_writer().expect("Unable to open writer");

    manager.on_update({
        let writer = log.get_writer().expect("Unable to open writer");
        let status_chan_tx = status_chan_tx.clone();
        let mut total_count = 0;
        let mut this_count = 0;
        let mut time_of_last_msg = Instant::now();
        move |time: SystemTime, update: &interface::EngineUpdate| {
            //println!("{:?}", update);
            writer.update(time, update.clone());
            this_count += 1;
            let duration = Instant::now() - time_of_last_msg;
            if duration >= Duration::from_secs(1) {
                total_count += this_count;
                status_chan_tx
                    .send(StatusMsg::UpdateCount {
                        count: total_count,
                        rate: this_count as f64 / duration.as_secs_f64(),
                    })
                    .unwrap();
                this_count = 0;
                time_of_last_msg += duration;
            }
        }
    });

    let writer = log.get_writer().expect("Unable to open writer");
    manager.on_event(
        move |time: SystemTime, event: &interface::Event| {
            writer.event(time, event.clone());
        });

    ctrlc::set_handler(move || status_chan_tx.send(StatusMsg::Terminate).unwrap()).unwrap();

    loop {
        match status_chan.recv_timeout(Duration::from_millis(1200)) {
            Ok(StatusMsg::Terminate) => break,
            Ok(StatusMsg::UpdateCount { count, rate }) => {
                println!("Connected! {} feed points received ({:.0}/s)", count, rate);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                println!("No new data");
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}
