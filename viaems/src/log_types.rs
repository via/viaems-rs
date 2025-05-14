use std::mem;

pub enum LogColumnValue {
    Time(i64),
    Int(u32),
    Float(f32),
    IntRange(u32, u32),
    FloatRange(f32, f32),
}

impl LogColumnValue {
    fn size(&self) -> usize {
        match self {
            LogColumnValue::Time(_) => 2,
            LogColumnValue::Int(_) => 1,
            LogColumnValue::Float(_) => 1,
            LogColumnValue::IntRange(_, _) => 2,
            LogColumnValue::FloatRange(_, _) => 2,
        }
    }
}

struct LogColumn {
    name: String,
    typ: LogColumnValue,
    row_offset: usize,
}

pub struct LogChunk2 {
    columns: Vec<LogColumn>,
    data: Vec<u32>,
    stride: usize,
    count: usize,
}

impl LogChunk2 {
    pub fn new(columns: Vec<(String, LogColumnValue)>) -> LogChunk2 {
        let mut chunk = LogChunk2 {
            columns: vec![],
            data: vec![],
            stride: 0,
            count: 0,
        };

        let timecolumn = LogColumn {
            name: "realtime_ns".to_owned(),
            typ: LogColumnValue::Time(0),
            row_offset: 0,
        };
        chunk.stride += timecolumn.typ.size();
        chunk.columns.push(timecolumn);

        for (name, typ) in columns {
            let col = LogColumn {
                name,
                typ,
                row_offset: chunk.stride,
            };
            chunk.stride += col.typ.size();
            chunk.columns.push(col);
        }

        chunk
    }

    fn insert_value(&mut self, value: &LogColumnValue) {
        match *value {
            LogColumnValue::Time(x) => {
                let bytes = x.to_le_bytes();
                self.data.push(u32::from_le_bytes(bytes[0..4].try_into().unwrap()));
                self.data.push(u32::from_le_bytes(bytes[4..8].try_into().unwrap()));
            },
            LogColumnValue::Int(x) => self.data.push(x),
            LogColumnValue::Float(x) => self.data.push(x.to_bits()),
            LogColumnValue::IntRange(x, y) => {
                self.data.push(x);
                self.data.push(y);
            },
            LogColumnValue::FloatRange(x, y) => {
                self.data.push(x.to_bits());
                self.data.push(y.to_bits());
            },
        }
    }

    fn insert(&mut self, time: i64, values: &[LogColumnValue]) {
        assert!(values.len() == self.columns.len() - 1);
        self.insert_value(&LogColumnValue::Time(time));
        for value in values {
            self.insert_value(value);
        }
        self.count += 1;
    }

    fn size(&self) -> usize {
        self.count
    }
}

mod tests {
    use super::*;

    #[inline(never)]
    fn setup() -> LogChunk2 {
        let columns = vec![
            ("rpm".to_string(), LogColumnValue::Int(0)),
            ("sensor.map".to_string(), LogColumnValue::Float(0.0)),
            ("sensor.tps".to_string(), LogColumnValue::IntRange(0, 100)),
        ];
        LogChunk2::new(columns)
    }

    #[test]
    fn test_chunk_insert() {
        let mut chunk = setup();

        chunk.insert(12345, &[ 
            LogColumnValue::Int(100),
            LogColumnValue::Float(50.0), 
            LogColumnValue::IntRange(0, 100) ]);
        assert_eq!(chunk.size(), 1);
        assert_eq!(chunk.data.len(), 6);

        assert_eq!(chunk.data[0], 12345);
        assert_eq!(chunk.data[1], 0);
        assert_eq!(chunk.data[2], 100);
        assert_eq!(f32::from_bits(chunk.data[3]), 50.0);
        assert_eq!(chunk.data[4], 0);
        assert_eq!(chunk.data[5], 100);

        chunk.insert(1012345, &[ 
            LogColumnValue::Int(101), 
            LogColumnValue::Float(51.0), 
            LogColumnValue::IntRange(1, 101) ]);
        assert_eq!(chunk.size(), 2);
        assert_eq!(chunk.data.len(), 12);
    }


}

