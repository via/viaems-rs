use viaems::{self, interface, connection};

use clap::{Parser, Subcommand};
use ctrlc;
use std::sync::mpsc;

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
  }
}


fn main() {
  let args = Args::parse();
  match args.command {
    CliCommands::Record{filename} => record(&filename, &args.udpsrc, &args.udpdest),
  }

}


fn wait_for_ctrlc() {
  let (tx, rx) = mpsc::channel::<()>();
  ctrlc::set_handler(move || tx.send(()).unwrap() ).unwrap();
  rx.recv().unwrap();
}



fn record(filename: &str, udpsrc: &str, udpdest: &str) {
    let conn = connection::UdpConnection::new(udpsrc, udpdest);
    let g = viaems::Manager::new(conn);
    let mut writer : Option<viaems::LogFeedWriter> = None;
    let filename = filename.to_owned();

    g.on_feed({
      move |time: i64, keys: &Vec<String>, vals: &Vec<interface::FeedValue>| {
        if writer.is_none() {
          writer = Some(viaems::LogFeedWriter::new(&filename, keys.clone()));
        }
        if let Some(w) = &mut writer {
          w.add(time, vals.clone());
        }
    }});

    let getcmd = interface::RequestMessage::Structure{id: 5};
    g.command(interface::Message::Request(getcmd),
      |resp: interface::ResponseValue| {
        println!("response: {:?}", resp);
       }
     );
  wait_for_ctrlc();
}
