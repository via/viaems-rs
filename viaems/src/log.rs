use std::sync::mpsc;
use std::thread;
use std::time::SystemTime;
use std::collections::HashMap;
use sqlite;

use crate::interface;

enum LogMessage {
    FeedPoint {
        time: SystemTime,
        values: Vec<interface::FeedValue>,
    },
    Terminate,
}

pub struct LogFeedWriter {
    tx: mpsc::Sender::<LogMessage>,
    handle: Option<thread::JoinHandle<()>>,
}

impl Drop for LogFeedWriter {
  fn drop(&mut self) {
    self.tx.send(LogMessage::Terminate).unwrap();
    let handle = self.handle.take();
    handle.unwrap().join().unwrap();
  }
}

impl LogFeedWriter {
    fn ensure_columns(keys: &Vec<String>, conn: &sqlite::Connection) {
      let mut current_keys : Vec<String> = vec![];
      for row in conn.prepare("PRAGMA TABLE_INFO(points);").unwrap()
          .into_iter().map(|r| r.unwrap()) {
              current_keys.push(row.read::<&str, _>("name").to_string());
      }

      if current_keys.len() == 0 {
        // Create table
          conn.execute("CREATE TABLE points (realtime_ns INTEGER);").unwrap();
      }

      for new_key in keys {
        if let None = current_keys.iter().find(|&x| x == new_key) {
          // Not currently there, alter table to add it
          conn.execute(format!("ALTER TABLE points ADD COLUMN '{}' REAL;",
          new_key)).unwrap();
        }
      }
    }

    pub fn new(filename: &str, keys: Vec<String>) -> LogFeedWriter {
        let (tx, rx) = mpsc::channel::<LogMessage>();

        let conn = sqlite::open(filename).unwrap();
        conn.execute("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; ").unwrap();
        LogFeedWriter::ensure_columns(&keys, &conn);

        let thr = thread::Builder::new().name("sqlite-feed-writer".to_string()).spawn(move || {

            let insert_cols = keys
                .iter()
                .map(|_| "?")
                .collect::<Vec<&str>>()
                .join(", ");
            let insert_names = keys
                .iter()
                .map(|x| format!("'{x}'"))
                .collect::<Vec<String>>()
                .join(", ");

            let mut stmt = conn.prepare(format!("insert into points (realtime_ns, {insert_names}) values (?, {insert_cols})")).unwrap();

            let mut remaining = 0; 
            while let Ok(val) = rx.recv() {
                match val {
                    LogMessage::FeedPoint{time, values} => {
                        if remaining == 0 {
                            conn.execute("BEGIN;").unwrap();
                            remaining = 5000;
                        }
                        LogFeedWriter::write(&mut stmt, time, values);
                        remaining -= 1;
                        if remaining == 0 {
                            conn.execute("COMMIT;").unwrap();
                        }
                    },
                    LogMessage::Terminate => break,
                }
            }
            conn.execute("COMMIT;").unwrap();
        }).unwrap();
        LogFeedWriter{ tx, handle: Some(thr) }
    }

    pub fn add(&self, time: SystemTime, values: Vec<interface::FeedValue>) {
      self.tx.send(LogMessage::FeedPoint{time, values}).unwrap();
    }

    fn write(stmt: &mut sqlite::Statement, time: SystemTime, vals: Vec<interface::FeedValue>) {
        let epoch_time : i64 = time.duration_since(SystemTime::UNIX_EPOCH).unwrap()
            .as_nanos().try_into().unwrap();
        stmt.reset().unwrap();
        stmt.bind((1, epoch_time)).unwrap();
        for (i, v) in vals.iter().enumerate() {
            match v { interface::FeedValue::Int(x) => stmt.bind((i + 2, *x as i64)),
                      interface::FeedValue::Float(x) => stmt.bind((i + 2, *x as f64)),
                      }.unwrap();
        }
        stmt.next().unwrap();
    }
}

pub struct LogReader {
  conn: sqlite::Connection,
  filename: String,
}

#[derive(Default)]
pub struct LogChunk {
    pub times: Vec<i64>,
    data: HashMap<String, Vec<f64>>,
}



impl LogReader {
    pub fn new(filename: &str) -> LogReader {
        let conn = sqlite::Connection::open_with_flags(filename,
            sqlite::OpenFlags::default().with_read_write().with_no_mutex()
            ).unwrap();
        LogReader{ 
            conn, 
            filename: filename.to_owned(),
        }
    }

    pub fn filename(&self) -> &str {
        &self.filename
    }

    pub fn keys(&self) -> Vec<String> {
        let mut keys : Vec<String> = vec![];
        for row in self.conn.prepare("PRAGMA TABLE_INFO(points);").unwrap().into_iter().map(|r| r.unwrap()) {
            keys.push(row.read::<&str, _>("name").to_string());
        }

        keys
    }

    pub fn get_range_row<F>(&self, start: SystemTime, stop: SystemTime, keys: &[&str], mut f: F) 
        where F: FnMut(sqlite::Row) -> ()
    {

        let start_ns = start.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_nanos() as i64;
        let stop_ns = stop.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_nanos() as i64;

        let key_cols = keys
            .iter()
            .map(|x| format!("`{x}`"))
            .collect::<Vec<String>>()
            .join(", ");

        let mut query = "SELECT realtime_ns, ".to_owned();
        query += &key_cols;
        query += " FROM points where realtime_ns > ? and realtime_ns < ? ORDER BY realtime_ns";

        let mut stmt = self.conn.prepare(query).unwrap();
        stmt.bind((1, start_ns)).unwrap();
        stmt.bind((2, stop_ns)).unwrap();
        for row in stmt.into_iter().map(|r| r.unwrap()) {
            f(row);
        }
    }


    pub fn get_range(&self, start: SystemTime, stop: SystemTime, keys: &[&str]) -> LogChunk {
        let start_ns = start.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_nanos() as i64;
        let stop_ns = stop.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_nanos() as i64;

        let key_cols = keys
            .iter()
            .map(|x| format!("`{x}`"))
            .collect::<Vec<String>>()
            .join(", ");

        let mut query = "SELECT realtime_ns, ".to_owned();
        query += &key_cols;
        query += " FROM points where realtime_ns > ? and realtime_ns < ? ORDER BY realtime_ns";

        let mut chunk = LogChunk::default();
        let mut stmt = self.conn.prepare(query).unwrap();
        stmt.bind((1, start_ns)).unwrap();
        stmt.bind((2, stop_ns)).unwrap();
        for row in stmt.into_iter().map(|r| r.unwrap()) {
            chunk.times.push(row.read::<i64, _>(0));
            for (idx, &k) in keys.iter().enumerate() {
                let value = row.try_read::<f64, _>(idx + 1)        // First try to parse f64
                    .or_else(|_| row.try_read::<i64, _>(idx + 1).map(|x| x as f64)) // Otherwise parse int and cast
                    .unwrap();
                if let Some(x) = chunk.data.get_mut(k) {
                    x.push(value);
                } else {
                    chunk.data.insert(k.to_owned(), vec![value]);
                }
            }

        }

        chunk
    }

}
