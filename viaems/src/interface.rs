#[derive(Debug, Clone)]

pub struct LoggableField {
    pub field_name: String,
    pub field_duckdb_typename: String,
}

pub trait LoggableMessage {
    fn get_loggable_fields(prefix: &str) -> Vec<LoggableField>;
    fn get_duckdb_value_list(&self) -> Vec<duckdb::types::Value>;
    fn get_f32_value_by_name(&self, name: &str) -> Option<f32>;
}

impl LoggableMessage for i32 {
    fn get_loggable_fields(path: &str) -> Vec<LoggableField> {
        vec![LoggableField {
            field_name: path.to_string(),
            field_duckdb_typename: "INTEGER".to_owned(),
        }]
    }
    fn get_duckdb_value_list(&self) -> Vec<duckdb::types::Value> {
        vec![duckdb::types::Value::Int(*self)]
    }
    fn get_f32_value_by_name(&self, _name: &str) -> Option<f32> {
        Some(*self as f32)
    }
}

impl LoggableMessage for f32 {
    fn get_loggable_fields(path: &str) -> Vec<LoggableField> {
        vec![LoggableField {
            field_name: path.to_string(),
            field_duckdb_typename: "FLOAT".to_owned(),
        }]
    }
    fn get_duckdb_value_list(&self) -> Vec<duckdb::types::Value> {
        vec![duckdb::types::Value::Float(*self)]
    }
    fn get_f32_value_by_name(&self, _name: &str) -> Option<f32> {
        Some(*self)
    }
}

impl LoggableMessage for bool {
    fn get_loggable_fields(path: &str) -> Vec<LoggableField> {
        vec![LoggableField {
            field_name: path.to_string(),
            field_duckdb_typename: "BOOLEAN".to_owned(),
        }]
    }
    fn get_duckdb_value_list(&self) -> Vec<duckdb::types::Value> {
        vec![duckdb::types::Value::Boolean(*self)]
    }
    fn get_f32_value_by_name(&self, _name: &str) -> Option<f32> {
        Some(if *self { 1.0 } else { 0.0 })
    }
}

impl LoggableMessage for u32 {
    fn get_loggable_fields(path: &str) -> Vec<LoggableField> {
        vec![LoggableField {
            field_name: path.to_string(),
            field_duckdb_typename: "UINTEGER".to_owned(),
        }]
    }
    fn get_duckdb_value_list(&self) -> Vec<duckdb::types::Value> {
        vec![duckdb::types::Value::UInt(*self)]
    }
    fn get_f32_value_by_name(&self, _name: &str) -> Option<f32> {
        Some(*self as f32)
    }
}

impl LoggableMessage for i64 {
    fn get_loggable_fields(path: &str) -> Vec<LoggableField> {
        vec![LoggableField {
            field_name: path.to_string(),
            field_duckdb_typename: "BIGINT".to_owned(),
        }]
    }
    fn get_duckdb_value_list(&self) -> Vec<duckdb::types::Value> {
        vec![duckdb::types::Value::BigInt(*self)]
    }
    fn get_f32_value_by_name(&self, _name: &str) -> Option<f32> {
        Some(*self as f32)
    }
}

impl<T> LoggableMessage for Option<T>
where
    T: LoggableMessage + Default + Copy,
{
    fn get_loggable_fields(path: &str) -> Vec<LoggableField> {
        <T as LoggableMessage>::get_loggable_fields(path)
    }
    fn get_duckdb_value_list(&self) -> Vec<duckdb::types::Value> {
        <T as LoggableMessage>::get_duckdb_value_list(&self.unwrap_or_default())
    }
    fn get_f32_value_by_name(&self, name: &str) -> Option<f32> {
        <T as LoggableMessage>::get_f32_value_by_name(&self.unwrap_or_default(), name)
    }
}

pub mod viaems {
    pub mod console {
        include!(concat!(env!("OUT_DIR"), "/viaems.console.rs"));
    }
}

pub use viaems::console::*;
