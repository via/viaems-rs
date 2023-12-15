use std::sync::mpsc;
use std::thread;
use sqlite;

use crate::interface;

struct FeedPoint {
  time: i64,
  values: Vec<interface::FeedValue>,
}

pub struct LogFeedWriter {
    tx: Option<mpsc::Sender::<FeedPoint>>,
    handle: Option<thread::JoinHandle<()>>,
}

impl Drop for LogFeedWriter {
  fn drop(&mut self) {
    self.tx.take(); // Explicitly drop the sender so that the recv thread can die

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
        let (tx, rx) = mpsc::channel::<FeedPoint>();

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
                if remaining == 0 {
                    conn.execute("BEGIN;").unwrap();
                    remaining = 5000;
                }
                LogFeedWriter::write(&mut stmt, val.time, val.values);

                remaining -= 1;
                if remaining == 0 {
                    conn.execute("COMMIT;").unwrap();
                }
            }
            conn.execute("COMMIT;").unwrap();
        }).unwrap();
        LogFeedWriter{ tx: Some(tx), handle: Some(thr) }
    }

    pub fn add(&self, time: i64, values: Vec<interface::FeedValue>) {
      if let Some(tx) = &self.tx {
        tx.send(FeedPoint{time, values}).unwrap();
      }
    }

    fn write(stmt: &mut sqlite::Statement, time: i64, vals: Vec<interface::FeedValue>) {
        stmt.reset().unwrap();
        stmt.bind((1, time)).unwrap();
        for (i, v) in vals.iter().enumerate() {
            match v { interface::FeedValue::Int(x) => stmt.bind((i + 2, *x as i64)),
                      interface::FeedValue::Float(x) => stmt.bind((i + 2, *x as f64)),
                      }.unwrap();
        }
        stmt.next().unwrap();
    }
}

