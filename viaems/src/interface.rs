use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use std::fmt;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "method")]
#[serde(rename_all = "lowercase")]
pub enum RequestMessage {
  Ping { id: i32 },
  Structure { id: i32 },
  Get { id: i32, path: StructurePath },
  Bootloader,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum StructurePathElement {
  ArrayIndex(u32),
  MapField(String),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StructurePath(Vec<StructurePathElement>);

impl StructurePath {
  pub fn new() -> StructurePath { StructurePath(vec![]) }
  pub fn add_str(mut self, s: &str) -> Self {
    self.0.push(StructurePathElement::MapField(s.to_string()));
    self
  }
  pub fn add_index(mut self, u: u32) -> Self {
    self.0.push(StructurePathElement::ArrayIndex(u));
    self
  }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StructureLeaf {
  _type: String,
  description: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum ResponseValue {
  Str(String),
  Float(f32),
  Int(u32),
  Bool(bool),
  Output(OutputValue),
  Array(Vec<ResponseValue>),
  Leaf(StructureLeaf),
  Map(HashMap<String, ResponseValue>),
  None
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "lowercase")]
#[serde(tag = "type")]
pub enum Event {
  Output{ outputs: u32 },
  Gpio{ outputs: u32 },
  Trigger{ pin: u32 },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "lowercase")]
#[serde(tag = "type")]
pub enum Message {
  Description { keys: Vec<String> },
  Feed { values: Vec<FeedValue> },
  Request(RequestMessage),
  Response{ id: i32, response: ResponseValue },
  Event{ time: u32, seq: u32, event: Event },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum FeedValue {
  Int(u32),
  Float(f32),
}

impl fmt::Display for FeedValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
          FeedValue::Int(x) => write!(f, "{}", x),
          FeedValue::Float(x) => write!(f, "{}", x),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "lowercase")]
pub enum OutputType {
  Ignition,
  Fuel,
  Disabled,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OutputValue {
  pin: u32,
#[serde(rename = "type")]
  output_type: OutputType,
  inverted: bool,
  angle: f32,
}
