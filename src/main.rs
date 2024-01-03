use viaems::{self, interface, connection};

use clap::{Parser, Subcommand};
use ctrlc;
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime};

#[derive(Parser, Debug)]
struct Args {
#[command(subcommand)]
  command: CliCommands,
#[arg(short = 's', long, default_value = "127.0.0.1:5556")]
  udpsrc: String,
#[arg(short = 'd', long, default_value = "127.0.0.1:5555")]
  udpdest: String,
}

#[derive(Subcommand, Debug)]
enum CliCommands {
  Record {
#[arg(default_value = "log.sq3")]
    filename: String, 
  },
  Bootloader,
}


fn main() {
  let args = Args::parse();
  match args.command {
    CliCommands::Record{filename} => record(&filename, &args.udpsrc, &args.udpdest),
    CliCommands::Bootloader => bootloader(),
  }

}

fn bootloader() {
}
    

enum StatusMsg {
    Terminate,
    FeedCount{count: u64, rate: f64},
}

fn record(filename: &str, udpsrc: &str, udpdest: &str) {
    let conn = Box::new(connection::UdpConnection::new(udpsrc, udpdest));
//    let uconn = connection::UsbConnection::new();
    let g = viaems::Manager::new(conn);
    let (status_chan_tx, status_chan) = mpsc::channel::<StatusMsg>();

    g.on_feed({
      let status_chan_tx = status_chan_tx.clone();
      let mut writer : Option<viaems::LogFeedWriter> = None;
      let filename = filename.to_owned();
      let mut total_count = 0;
      let mut this_count = 0;
      let mut time_of_last_msg = Instant::now();
      move |time: SystemTime, keys: &Vec<String>, vals: &Vec<interface::FeedValue>| {
        if writer.is_none() {
          writer = Some(viaems::LogFeedWriter::new(&filename, keys.clone()));
        }
        if let Some(w) = &mut writer {
          w.add(time, vals.clone());
        }
        this_count += 1;
        let duration = Instant::now() - time_of_last_msg;
        if duration >= Duration::from_secs(1) {
            total_count += this_count;
            status_chan_tx.send(StatusMsg::FeedCount{
                count: total_count,
                rate: this_count as f64 / duration.as_secs_f64(),
            }).unwrap();
            this_count = 0;
            time_of_last_msg += duration;
        }

    }});

    let getcmd = interface::RequestMessage::Structure{id: 5};
    g.command(interface::Message::Request(getcmd),
      |resp: interface::ResponseValue| {
        println!("response: {:?}", resp);
       }
     );

    ctrlc::set_handler(move || status_chan_tx.send(StatusMsg::Terminate).unwrap() ).unwrap();

    loop {
        match status_chan.recv_timeout(Duration::from_millis(1000)) {
            Ok(StatusMsg::Terminate) => break,
            Ok(StatusMsg::FeedCount{count, rate}) => {
                println!("Connected! {} feed points received ({:.0}/s)", count, rate);
            },
            Err(mpsc::RecvTimeoutError::Timeout) => {
                println!("No new data");
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}
