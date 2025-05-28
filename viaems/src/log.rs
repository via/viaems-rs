use duckdb;
use duckdb::arrow::array::{AsArray, PrimitiveArray};
use duckdb::arrow::datatypes;
use std::collections::HashMap;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, SystemTime};

use crate::interface::{self, FeedValue};

enum LogMessage {
    FeedPoint {
        time: SystemTime,
        values: Vec<interface::FeedValue>,
    },
    Terminate,
}

pub struct LogFeedWriter {
    tx: mpsc::Sender<LogMessage>,
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
    fn ensure_columns(
        keys: &Vec<String>,
        values: &Vec<interface::FeedValue>,
        conn: &duckdb::Connection,
    ) {
        let current_keys: Vec<String> = conn
            .prepare("PRAGMA TABLE_INFO(points);")
            .ok()
            .and_then(|mut s| {
                Some(
                    s.query_map([], |r| Ok(r.get::<_, String>("name").unwrap()))
                        .unwrap()
                        .map(|x| x.unwrap())
                        .collect(),
                )
            })
            .unwrap_or(vec![]);

        if current_keys.len() == 0 {
            // Create table
            conn.execute("CREATE TABLE points (realtime_ns BIGINT);", [])
                .unwrap();
        }

        for (new_key, val) in std::iter::zip(keys, values) {
            if let None = current_keys.iter().find(|&x| x == new_key) {
                // Not currently there, alter table to add it
                let col_type = if let interface::FeedValue::Int(_) = val {
                    "UINTEGER"
                } else {
                    "FLOAT"
                };
                conn.execute(
                    &format!(
                        "ALTER TABLE points ADD COLUMN \"{}\" {};",
                        new_key, col_type
                    ),
                    [],
                )
                .unwrap();
            }
        }
    }

    pub fn new(
        filename: &str,
        keys: Vec<String>,
        values: Vec<interface::FeedValue>,
    ) -> Result<LogFeedWriter, duckdb::Error> {
        let (tx, rx) = mpsc::channel::<LogMessage>();

        let conn = duckdb::Connection::open(filename)?;

        LogFeedWriter::ensure_columns(&keys, &values, &conn);

        let thr = thread::Builder::new()
            .name("sqlite-feed-writer".to_string())
            .spawn(move || {
                let mut appender = conn.appender("points").unwrap();
                let mut count = 0;

                while let Ok(val) = rx.recv() {
                    match val {
                        LogMessage::FeedPoint { time, values } => {
                            LogFeedWriter::write(&mut appender, time, values);
                            count += 1;
                            if count > 10000 {
                                appender.flush().unwrap();
                                count = 0;
                            }
                        }
                        LogMessage::Terminate => {
                            appender.flush().unwrap();
                            break;
                        }
                    }
                }
            })
            .unwrap();
        Ok(LogFeedWriter {
            tx,
            handle: Some(thr),
        })
    }

    pub fn add(&self, time: SystemTime, values: Vec<interface::FeedValue>) {
        self.tx
            .send(LogMessage::FeedPoint { time, values })
            .unwrap();
    }

    fn write(appender: &mut duckdb::Appender, time: SystemTime, vals: Vec<interface::FeedValue>) {
        let epoch_time: i64 = time
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            .try_into()
            .unwrap();

        let mut params_list = vec![duckdb::types::Value::BigInt(epoch_time)];
        vals.iter().for_each(|v| match v {
            interface::FeedValue::Int(x) => params_list.push(duckdb::types::Value::UInt(*x)),
            interface::FeedValue::Float(x) => params_list.push(duckdb::types::Value::Float(*x)),
        });

        appender
            .append_row(duckdb::appender_params_from_iter(params_list))
            .unwrap();
    }
}

pub struct LogReader {
    conn: duckdb::Connection,
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
        let conf = duckdb::Config::default()
            .enable_autoload_extension(false)
            .unwrap()
            .access_mode(duckdb::AccessMode::ReadOnly)
            .unwrap();
        let conn = duckdb::Connection::open_with_flags(filename, conf).unwrap();
        conn.execute("SET autoinstall_known_extensions = false;", [])
            .unwrap();
        conn.execute("SET autoload_known_extensions = false;", [])
            .unwrap();
        conn.execute("SET lock_configuration = true;", []).unwrap();
        LogReader {
            conn,
            filename: filename.to_owned(),
        }
    }

    pub fn filename(&self) -> &str {
        &self.filename
    }

    pub fn keys(&self) -> Vec<String> {
        self.conn
            .prepare("PRAGMA TABLE_INFO(points);")
            .unwrap()
            .query_map([], |row| row.get::<_, String>("name"))
            .unwrap()
            .flatten()
            .skip(1)
            .collect()
    }

    pub fn range_foreach<F>(&self, start: SystemTime, stop: SystemTime, keys: &[&str], mut f: F)
    where
        F: FnMut(i64, &[f64]) -> bool,
    {
        let start_ns = start
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as i64;
        let stop_ns = stop
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as i64;

        let key_cols = keys
            .iter()
            .map(|x| format!("\"{x}\""))
            .collect::<Vec<String>>()
            .join(", ");

        let mut query = "SELECT realtime_ns, ".to_owned();
        query += &key_cols;
        query += &format!(
            " FROM points where realtime_ns > {} and realtime_ns < {} ORDER BY realtime_ns",
            start_ns, stop_ns
        );

        let mut stmt = self.conn.prepare(&query).unwrap();
        let batch_iterator = stmt.query_arrow([]).unwrap();
        let mut values: Vec<f64> = vec![];

        for batch in batch_iterator {
            let cols = batch.columns();
            let times = cols[0].as_primitive::<datatypes::Int64Type>();
            let rest: Vec<&PrimitiveArray<datatypes::Float32Type>> = cols[1..]
                .iter()
                .map(|s| s.as_primitive::<datatypes::Float32Type>())
                .collect();
            for idx in 0..batch.num_rows() {
                values.clear();
                for col in &rest {
                    values.push(col.value(idx) as f64)
                }
                if !f(times.value(idx), values.as_slice()) {
                    break;
                };
            }
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
        let mut stmt = self.conn.prepare(query).unwrap();
        if let Ok(Some(r)) = stmt.query([]).unwrap().next() {
            let time_ns = r.get::<_, i64>(0).unwrap();
            let duration = Duration::from_nanos(time_ns.try_into().unwrap());
            let time = SystemTime::UNIX_EPOCH.checked_add(duration).unwrap();
            return Some(time);
        } else {
            return None;
        }
    }

    pub fn get_latest_time(&self) -> Option<SystemTime> {
        let query = "SELECT realtime_ns from points order by realtime_ns desc limit 1";
        let mut stmt = self.conn.prepare(query).unwrap();
        if let Ok(Some(r)) = stmt.query([]).unwrap().next() {
            let time_ns = r.get::<_, i64>(0).unwrap();
            let duration = Duration::from_nanos(time_ns.try_into().unwrap());
            let time = SystemTime::UNIX_EPOCH.checked_add(duration).unwrap();
            return Some(time);
        } else {
            return None;
        }
    }
}
