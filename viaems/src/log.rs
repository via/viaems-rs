use std::sync::mpsc;
use std::thread;
use std::time::{SystemTime, Duration};
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

#[derive(Default, Clone)]
pub struct LogChunk {
    pub keys: Vec<String>,
    pub times: Vec<i64>,
    pub data: Vec<Vec<f64>>,
}

impl LogChunk {
    pub fn new(keys: &[&str]) -> LogChunk {
        let mut chunk = LogChunk {
            keys: keys.iter().map(|x| x.to_string()).collect(),
            times: vec![],
            data: vec![],
        };
        chunk.data.resize(keys.len(), vec![]);
        chunk
    }

    pub fn add(&mut self, time: i64, values: &[f64]) {
        self.times.push(time);
        for (idx, v) in values.iter().enumerate() {
            self.data[idx].push(*v)
        }
    }

    pub fn clear(&mut self) {
        self.times.clear();
        for col in &mut self.data {
            col.clear();
        }
    }
}



impl LogReader {
    pub fn new(filename: &str) -> LogReader {
        let conn = sqlite::Connection::open_with_flags(filename,
            sqlite::OpenFlags::default().with_read_only().with_no_mutex()
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

    pub fn get_range_count(&self, start: SystemTime, stop: SystemTime) -> usize {
        let start_ns = start.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_nanos() as i64;
        let stop_ns = stop.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_nanos() as i64;

        let query = "SELECT count(realtime_ns) FROM points where realtime_ns > ? and realtime_ns < ?";
        let mut stmt = self.conn.prepare(query).unwrap();
        stmt.bind((1, start_ns)).unwrap();
        stmt.bind((2, stop_ns)).unwrap();
        if let Some(Ok(row)) = stmt.into_iter().next() {
            row.read::<i64, _>(0).try_into().expect("row count not valid number")
        } else {
            0
        }
    }

    pub fn range_foreach<F>(&self, start: SystemTime, stop: SystemTime, keys: &[&str], mut f: F) 
        where F: FnMut(i64, &Vec<f64>) -> bool
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
        let mut values = vec![];
        for row in stmt.into_iter().map(|r| r.unwrap()) {
            let time = row.read::<i64, _>(0);
            values.clear();
            for i in 1..=keys.len() {
                let value = row
                    .try_read::<f64, _>(i)
                    .or_else(|_| row.try_read::<i64, _>(i).map(|x| x as f64))
                    .unwrap();
                values.push(value);
            }
            if !f(time, &values) { break; }
        }
    }


    pub fn get_range(&self, start: SystemTime, stop: SystemTime, keys: &[&str]) -> LogChunk {

        let mut chunk = LogChunk::new(keys);
        self.range_foreach(start, stop, keys, |time, values| {
            chunk.add(time, values);
            true
        });

        chunk
    }

    // TODO Learn out how to use ? shorthand to get rid of the unwraps
    pub fn get_earliest_time(&self) -> Option<SystemTime> {
        let query = "SELECT realtime_ns from points order by realtime_ns asc limit 1";
        let stmt = self.conn.prepare(query).unwrap();
        if let Some(Ok(r)) = stmt.into_iter().next() {
            let time_ns = r.try_read::<i64, _>(0).unwrap();
            let duration = Duration::from_nanos(time_ns.try_into().unwrap());
            let time = SystemTime::UNIX_EPOCH.checked_add(duration).unwrap();
            return Some(time);
        } else {
            return None;
        }
    }

    pub fn get_latest_time(&self) -> Option<SystemTime> {
        let query = "SELECT realtime_ns from points order by realtime_ns desc limit 1";
        let stmt = self.conn.prepare(query).unwrap();
        if let Some(Ok(r)) = stmt.into_iter().next() {
            let time_ns = r.try_read::<i64, _>(0).unwrap();
            let duration = Duration::from_nanos(time_ns.try_into().unwrap());
            let time = SystemTime::UNIX_EPOCH.checked_add(duration).unwrap();
            return Some(time);
        } else {
            return None;
        }
    }
}
