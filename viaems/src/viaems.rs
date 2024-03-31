use std::thread;
use std::sync::{Mutex, Arc};
use std::sync::mpsc::RecvTimeoutError;
use std::time::Duration;

use crate::interface;
use crate::connection;

