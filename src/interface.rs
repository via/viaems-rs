use serde::{Serialize, Deserialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "method")]
#[serde(rename_all = "lowercase")]
pub enum RequestMessage {
  Ping { id: u32 },
  Structure { id: u32 },
  Get { id: u32, path: StructurePath },
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
pub enum StructurePathElement {
  ArrayIndex(u32),
  MapField(String),
}

#[derive(Serialize, Deserialize, Debug)]
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

#[derive(Serialize, Deserialize, Debug)]
pub struct StructureLeaf {
  _type: String,
  description: String,
}

#[derive(Serialize, Deserialize, Debug)]
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

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
#[serde(tag = "type")]
pub enum Message {
  Description { keys: Vec<String> },
  Feed { values: Vec<FeedValue> },
  Request(RequestMessage),
  Response{ id: u32, response: ResponseValue },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum FeedValue {
  Int(u32),
  Float(f32),
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
pub enum OutputType {
  Ignition,
  Fuel,
  Disabled,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct OutputValue {
  pin: u32,
#[serde(rename = "type")]
  output_type: OutputType,
  inverted: bool,
  angle: f32,
}
