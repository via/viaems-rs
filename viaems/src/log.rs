use duckdb;
use duckdb::arrow::array::RecordBatch;
use duckdb::arrow::datatypes::{self, Schema, SchemaBuilder, SchemaRef};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, SystemTime};
use std::collections::HashMap;

use crate::interface;

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
    Update {
        time: SystemTime,
        update: interface::EngineUpdate,
    },
    Terminate,
}

pub struct UpdateWriter {
    tx: mpsc::Sender<LogMessage>,
    handle: Option<thread::JoinHandle<()>>,
}

impl Drop for UpdateWriter {
    fn drop(&mut self) {
        self.tx.send(LogMessage::Terminate).unwrap();
        let handle = self.handle.take();
        handle.unwrap().join().unwrap();
    }
}

impl UpdateWriter {
    fn ensure_columns(conn: &duckdb::Connection) -> Result<()> {
        let mut existing_columns = HashMap::new();
        let mut message_fields = <i64 as interface::LoggableMessage>::get_loggable_fields("realtime_ns");
        message_fields.append(&mut <interface::EngineUpdate as interface::LoggableMessage>::get_loggable_fields(""));
        let message_fields = message_fields;

        if let Ok(stmt) = &mut conn.prepare("DESCRIBE TABLE points;") {
            for result in stmt.query([])?.and_then(|r| -> Result<_> {
                let col_name: String = r.get("column_name")?;
                let col_type: String = r.get("column_type")?;
                Ok((col_name, col_type))
            }) {
                let (n, t) = result.unwrap();
                existing_columns.entry(n).or_insert(t);
            }

            for interface::LoggableField{field_name, field_duckdb_typename} in &message_fields {
                if let Some(existing_type) = existing_columns.get(field_name) {
                    if existing_type != field_duckdb_typename {
                    return Err(Error::FeedKeysMismatch(field_name.to_string()));
                    }
                } else {
                    // TODO add the column
                }
            }
        } else {
            // Table did not exist or new database, go ahead and create points
            let mut query = "CREATE TABLE points (".to_owned();
            for interface::LoggableField{field_name, field_duckdb_typename} in &message_fields {
                query += &format!("\"{}\" {}, ", field_name, field_duckdb_typename);
            }

            query += ");";
            conn.execute(&query, [])?;
        }

        Ok(())
    }

    pub fn new(conn: duckdb::Connection) -> Result<UpdateWriter> {
        UpdateWriter::ensure_columns(&conn)?;

        let (tx, rx) = mpsc::channel::<LogMessage>();

        let thr = thread::Builder::new()
            .name("duckdb-update-writer".to_string())
            .spawn(move || {
                let mut count = 0;

                let mut appender = conn.appender("points").unwrap();
                while let Ok(val) = rx.recv() {
                    match val {
                        LogMessage::Update { time, update } => {
                            UpdateWriter::write(&mut appender, time, &update);
                            count += 1;
                            if count > 25000 {
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
        Ok(UpdateWriter {
            tx,
            handle: Some(thr),
        })

    }

    pub fn add(&self, time: SystemTime, update: interface::EngineUpdate) {
        self.tx
            .send(LogMessage::Update { time, update })
            .unwrap();
    }

    fn write(appender: &mut duckdb::Appender, time: SystemTime, update: &interface::EngineUpdate) {
        let epoch_time: i64 = time
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            .try_into()
            .unwrap();

        let mut params_list = interface::LoggableMessage::get_duckdb_value_list(&epoch_time);
        params_list.append(&mut interface::LoggableMessage::get_duckdb_value_list(update));

        appender
            .append_row(duckdb::appender_params_from_iter(params_list))
            .unwrap();
    }
}

pub struct Log {
    conn: duckdb::Connection,
    filename: String,
}

impl Log {
    pub fn new(filename: &str) -> Log {
        let conf = duckdb::Config::default()
            .max_memory("2GB")
            .unwrap()
            .enable_autoload_extension(false)
            .unwrap()
            .access_mode(duckdb::AccessMode::ReadWrite)
            .unwrap();
        let conn = duckdb::Connection::open_with_flags(filename, conf).unwrap();
        conn.execute("SET autoinstall_known_extensions = false;", [])
            .unwrap();
        conn.execute("SET autoload_known_extensions = false;", [])
            .unwrap();
        conn.execute("SET lock_configuration = true;", []).unwrap();
        Log {
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

    pub fn get_writer(&self) -> Result<UpdateWriter> {
        let conn = self.conn.try_clone()?;
        UpdateWriter::new(conn)

    }

    pub fn try_clone(&self) -> Result<Log> {
        let conn = self.conn.try_clone()?;
        Ok(Log { conn, filename: self.filename.clone() })
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

        let schema_query = query.clone() + " LIMIT 0";
        let mut schema_stmt = self.conn.prepare(&schema_query).unwrap();
        let schema_result = schema_stmt.query_arrow([]).unwrap();
        let full_schema = schema_result.get_schema();

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
        let mut stream = stmt.stream_arrow([], projected_schema)?;
        let mut count = 0;

        while let Some(batch) = stream.next() {
            count += batch.num_rows();
            f(&batch);
        }
        //println!("{:?} query: {}, {} rows", stop.duration_since(start).unwrap_or_default(), query, count);
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
