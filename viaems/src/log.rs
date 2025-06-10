use duckdb;
use duckdb::arrow::array::{AsArray, PrimitiveArray, RecordBatch, StructArray};
use duckdb::arrow::datatypes::{self, Schema, SchemaBuilder, SchemaRef};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, SystemTime};

use crate::interface::{self, FeedValue};

#[derive(Debug)]
pub enum Error {
    DuckDBError(duckdb::Error, Option<String>),
    ArrowError(duckdb::arrow::error::ArrowError),
    FeedKeysMismatch(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::DuckDBError(dbe, Some(x)) => write!(f, "duckdb: {} (){})", x, dbe),
            Error::DuckDBError(dbe, None) => write!(f, "duckdb: {}", dbe),
            Error::FeedKeysMismatch(x) => write!(f, "log columns mismatch: {}", x),
            Error::ArrowError(x) => write!(f, "arrow: {}", x),
        }
    }
}

impl From<duckdb::Error> for Error {
    fn from(value: duckdb::Error) -> Self {
        Error::DuckDBError(value, None)
    }
}

impl From<duckdb::arrow::error::ArrowError> for Error {
    fn from(value: duckdb::arrow::error::ArrowError) -> Self {
        Error::ArrowError(value)
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

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
    ) -> Result<()> {
        let mut columns = vec![];
        if let Ok(stmt) = &mut conn.prepare("DESCRIBE TABLE points;") {
            for result in stmt.query([])?.and_then(|r| -> Result<_> {
                let col_name: String = r.get("column_name")?;
                let col_type: String = r.get("column_type")?;
                Ok((col_name, col_type))
            }) {
                columns.push(result?);
            }

            if columns[0].0 != "realtime_ns" && columns[0].1 != "BIGINT" {
                return Err(Error::FeedKeysMismatch(
                    "realtime_ns is not BIGINT".to_owned(),
                ));
            }
            columns.remove(0); // Get rid of time column for comparison

            for (idx, (k, v)) in std::iter::zip(keys, values).enumerate() {
                let kt = match v {
                    interface::FeedValue::Int(_) => "UINTEGER",
                    interface::FeedValue::Float(_) => "FLOAT",
                };
                if columns[idx].0 != *k || columns[idx].1 != kt {
                    return Err(Error::FeedKeysMismatch(columns[idx].0.clone()));
                }
                if columns.len() != keys.len() {
                    return Err(Error::FeedKeysMismatch(
                        "different number of columns".to_owned(),
                    ));
                }
            }
        } else {
            // Table did not exist or new database, go ahead and create points
            let mut query = "CREATE TABLE points (realtime_ns BIGINT, ".to_owned();
            for (new_key, val) in std::iter::zip(keys, values) {
                let col_type = if let interface::FeedValue::Int(_) = val {
                    "UINTEGER"
                } else {
                    "FLOAT"
                };
                query += &format!("\"{}\" {}, ", new_key, col_type);
            }

            query += ");";
            conn.execute(&query, [])?;
        }

        Ok(())
    }

    pub fn new(
        filename: &str,
        keys: Vec<String>,
        values: Vec<interface::FeedValue>,
    ) -> Result<LogFeedWriter> {
        let (tx, rx) = mpsc::channel::<LogMessage>();

        let conn = duckdb::Connection::open(filename)?;
        conn.execute("SET autoinstall_known_extensions = false;", [])?;
        conn.execute("SET autoload_known_extensions = false;", [])?;
        conn.execute("SET lock_configuration = true;", [])?;

        LogFeedWriter::ensure_columns(&keys, &values, &conn)?;

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

impl LogReader {
    pub fn new(filename: &str) -> LogReader {
        let conf = duckdb::Config::default()
            .max_memory("2GB")
            .unwrap()
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
            .prepare("DESCRIBE TABLE points;")
            .unwrap()
            .query_map([], |row| row.get::<_, String>("column_name"))
            .unwrap()
            .flatten()
            .skip(1)
            .collect()
    }

    fn schema(&self) -> Result<Schema> {
        let mut stmt = self.conn.prepare("DESCRIBE TABLE points;")?;
        let mut builder = SchemaBuilder::new();
        for result in stmt.query([])?.and_then(|r| -> Result<_> {
            let col_name: String = r.get("column_name")?;
            let col_type: String = r.get("column_type")?;
            Ok((col_name, col_type))
        }) {
            let (col_name, col_type) = result?;
            let schema_type = match col_type.as_str() {
                "BIGINT" => datatypes::DataType::Int64,
                "UINTEGER" => datatypes::DataType::UInt32,
                "FLOAT" => datatypes::DataType::Float32,
                "DOUBLE" => datatypes::DataType::Float64,
                _ => {
                    return Err(Error::FeedKeysMismatch(format!(
                        "unknown type in log for {}: {}",
                        col_name, col_type
                    )));
                }
            };
            builder.push(datatypes::Field::new(col_name, schema_type, true));
        }

        Ok(builder.finish())
    }

    pub fn query_arrow<F>(
        &self,
        start: SystemTime,
        stop: SystemTime,
        keys: &[&str],
        mut f: F,
    ) -> Result<()>
    where
        F: FnMut(&RecordBatch),
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
            " FROM points where realtime_ns > {} and realtime_ns < {}",
            start_ns, stop_ns
        );

        let full_schema = self.schema()?;
        let mut idxs = vec![0 as usize]; // always include realtime_ns
        for k in keys {
            if let Some(i) = full_schema.fields.iter().position(|f| f.name() == k) {
                idxs.push(i);
            } else {
                return Err(Error::FeedKeysMismatch("key not found".to_owned()));
            }
        }
        let projected_schema = SchemaRef::new(full_schema.project(&idxs)?);
        let mut stmt = self.conn.prepare(&query)?;
        println!("query: {}", query);
        let mut stream = stmt.stream_arrow([], projected_schema)?;

        while let Some(batch) = stream.next() {
            f(&batch);
        }
        Ok(())
    }

    pub fn point_count_in_range(&self, start: SystemTime, stop: SystemTime) -> Result<i64> {
        let start_ns = start
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as i64;
        let stop_ns = stop
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as i64;
        let query = "SELECT count(*) from points where realtime_ns > ? and realtime_ns < ?;";

        let result = self
            .conn
            .query_row(query, [start_ns, stop_ns], |r| r.get::<_, i64>(0))?;

        Ok(result)
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
