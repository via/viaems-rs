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

const TABLE_SCHEMA : &'static [(&str, &str)] = &[
    ("realtime_ns", "BIGINT"),
    ("cputime", "UINTEGER"),
    ("sensors.map", "FLOAT"),
    ("sensors.iat", "FLOAT"),
    ("sensors.clt", "FLOAT"),
    ("sensors.brv", "FLOAT"),
    ("sensors.tps", "FLOAT"),
    ("sensors.aap", "FLOAT"),
    ("sensors.frt", "FLOAT"),
    ("sensors.ego", "FLOAT"),
    ("sensors.frp", "FLOAT"),
    ("sensors.eth", "FLOAT"),
    ("sensors.knock1", "FLOAT"),
    ("sensors.knock2", "FLOAT"),

    ("sensors.map_fault", "INTEGER"),
    ("sensors.iat_fault", "INTEGER"),
    ("sensors.clt_fault", "INTEGER"),
    ("sensors.brv_fault", "INTEGER"),
    ("sensors.tps_fault", "INTEGER"),
    ("sensors.aap_fault", "INTEGER"),
    ("sensors.frt_fault", "INTEGER"),
    ("sensors.ego_fault", "INTEGER"),
    ("sensors.frp_fault", "INTEGER"),
    ("sensors.eth_fault", "INTEGER"),

    ("sensors.map_rate", "FLOAT"),
    ("sensors.iat_rate", "FLOAT"),
    ("sensors.clt_rate", "FLOAT"),
    ("sensors.brv_rate", "FLOAT"),
    ("sensors.tps_rate", "FLOAT"),
    ("sensors.aap_rate", "FLOAT"),
    ("sensors.frt_rate", "FLOAT"),
    ("sensors.ego_rate", "FLOAT"),
    ("sensors.frp_rate", "FLOAT"),
    ("sensors.eth_rate", "FLOAT"),

    ("position.time", "UINTEGER"),
    ("position.valid_before_timestamp", "UINTEGER"),
    ("position.has_position", "BOOLEAN"),
    ("position.synced", "BOOLEAN"),
    ("position.loss_cause", "INTEGER"),
    ("position.last_angle", "FLOAT"),
    ("position.instantaneous_rpm", "FLOAT"),
    ("position.average_rpm", "FLOAT"),

    ("calculations.advance", "FLOAT"),
    ("calculations.dwell_us", "FLOAT"),
    ("calculations.fuel_us", "FLOAT"),
    ("calculations.airmass_per_cycle", "FLOAT"),
    ("calculations.fuelvol_per_cycle", "FLOAT"),
    ("calculations.tipin_percent", "FLOAT"),
    ("calculations.injector_dead_time", "FLOAT"),
    ("calculations.pulse_width_correction", "FLOAT"),
    ("calculations.lambda", "FLOAT"),
    ("calculations.ve", "FLOAT"),
    ("calculations.engine_temp_enrichment", "FLOAT"),

    ("calculations.rpm_limit_cut", "BOOLEAN"),
    ("calculations.boost_cut", "BOOLEAN"),
    ("calculations.fuel_overduty_cut", "BOOLEAN"),
    ("calculations.dwell_overduty_cut", "BOOLEAN"),
];

impl UpdateWriter {
    fn ensure_columns(conn: &duckdb::Connection) -> Result<()> {
        let mut existing_columns = HashMap::new();
        if let Ok(stmt) = &mut conn.prepare("DESCRIBE TABLE points;") {
            for result in stmt.query([])?.and_then(|r| -> Result<_> {
                let col_name: String = r.get("column_name")?;
                let col_type: String = r.get("column_type")?;
                Ok((col_name, col_type))
            }) {
                let (n, t) = result.unwrap();
                existing_columns.entry(n).or_insert(t);
            }

            for (col_name, col_type) in TABLE_SCHEMA {
                if let Some(existing_type) = existing_columns.get(*col_name) {
                    if existing_type != col_type {
                    return Err(Error::FeedKeysMismatch(col_name.to_string()));
                    }
                } else {
                    // TODO add the column
                }
            }
        } else {
            // Table did not exist or new database, go ahead and create points
            let mut query = "CREATE TABLE points (".to_owned();
            for (new_key, new_type) in TABLE_SCHEMA {
                query += &format!("\"{}\" {}, ", new_key, new_type);
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
                            if count > 2000 {
                                appender.flush().unwrap();
                                println!("Wrote 2000");
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

        let header = update.header.unwrap_or_default();
        let sensors = update.sensors.unwrap_or_default();
        let position = update.position.unwrap_or_default();
        let calcs = update.calculations.unwrap_or_default();

        let params_list = vec![
            duckdb::types::Value::BigInt(epoch_time),
            duckdb::types::Value::UInt(header.timestamp),
            duckdb::types::Value::Float(sensors.map),
            duckdb::types::Value::Float(sensors.iat),
            duckdb::types::Value::Float(sensors.clt),
            duckdb::types::Value::Float(sensors.brv),
            duckdb::types::Value::Float(sensors.tps),
            duckdb::types::Value::Float(sensors.aap),
            duckdb::types::Value::Float(sensors.frt),
            duckdb::types::Value::Float(sensors.ego),
            duckdb::types::Value::Float(sensors.frp),
            duckdb::types::Value::Float(sensors.eth),
            duckdb::types::Value::Float(sensors.knock1),
            duckdb::types::Value::Float(sensors.knock2),

            duckdb::types::Value::Int(sensors.map_fault),
            duckdb::types::Value::Int(sensors.iat_fault),
            duckdb::types::Value::Int(sensors.clt_fault),
            duckdb::types::Value::Int(sensors.brv_fault),
            duckdb::types::Value::Int(sensors.tps_fault),
            duckdb::types::Value::Int(sensors.aap_fault),
            duckdb::types::Value::Int(sensors.frt_fault),
            duckdb::types::Value::Int(sensors.ego_fault),
            duckdb::types::Value::Int(sensors.frp_fault),
            duckdb::types::Value::Int(sensors.eth_fault),

            duckdb::types::Value::Float(sensors.map_rate),
            duckdb::types::Value::Float(sensors.iat_rate),
            duckdb::types::Value::Float(sensors.clt_rate),
            duckdb::types::Value::Float(sensors.brv_rate),
            duckdb::types::Value::Float(sensors.tps_rate),
            duckdb::types::Value::Float(sensors.aap_rate),
            duckdb::types::Value::Float(sensors.frt_rate),
            duckdb::types::Value::Float(sensors.ego_rate),
            duckdb::types::Value::Float(sensors.frp_rate),
            duckdb::types::Value::Float(sensors.eth_rate),

            duckdb::types::Value::UInt(position.time),
            duckdb::types::Value::UInt(position.valid_before_timestamp),
            duckdb::types::Value::Boolean(position.has_position),
            duckdb::types::Value::Boolean(position.synced),
            duckdb::types::Value::Int(position.loss_cause),
            duckdb::types::Value::Float(position.last_angle),
            duckdb::types::Value::Float(position.instantaneous_rpm),
            duckdb::types::Value::Float(position.average_rpm),

            duckdb::types::Value::Float(calcs.advance),
            duckdb::types::Value::Float(calcs.dwell_us),
            duckdb::types::Value::Float(calcs.fuel_us),
            duckdb::types::Value::Float(calcs.airmass_per_cycle),
            duckdb::types::Value::Float(calcs.fuelvol_per_cycle),
            duckdb::types::Value::Float(calcs.tipin_percent),
            duckdb::types::Value::Float(calcs.injector_dead_time),
            duckdb::types::Value::Float(calcs.pulse_width_correction),
            duckdb::types::Value::Float(calcs.lambda),
            duckdb::types::Value::Float(calcs.ve),
            duckdb::types::Value::Float(calcs.engine_temp_enrichment),

            duckdb::types::Value::Boolean(calcs.rpm_limit_cut),
            duckdb::types::Value::Boolean(calcs.boost_cut),
            duckdb::types::Value::Boolean(calcs.fuel_overduty_cut),
            duckdb::types::Value::Boolean(calcs.dwell_overduty_cut),
        ];

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
                "BOOLEAN" => datatypes::DataType::Boolean,
                "BIGINT" => datatypes::DataType::Int64,
                "UINTEGER" => datatypes::DataType::UInt32,
                "INTEGER" => datatypes::DataType::Int32,
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
