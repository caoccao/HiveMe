/*
* Copyright (c) 2026. caoccao.com Sam Cao
* All rights reserved.

* Licensed under the Apache License, Version 2.0 (the "License");
* you may not use this file except in compliance with the License.
* You may obtain a copy of the License at

* http://www.apache.org/licenses/LICENSE-2.0

* Unless required by applicable law or agreed to in writing, software
* distributed under the License is distributed on an "AS IS" BASIS,
* WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
* See the License for the specific language governing permissions and
* limitations under the License.
*/

//! The collapsible JSON tree of `data` and of a raw JSON payload, `JsonTree` in
//! `MessageView.tsx`: the top level is open and every branch below it starts closed.
//!
//! Which branches the user opened or closed is kept per message as a map from a node's
//! path to its state, so that the default needs no entry at all.
//!
//! The payload is read again into [`Json`], which keeps the keys of an object in the
//! order they were written, as `Object.entries` does in the GUI. `serde_json::Value`
//! sorts them.

use std::collections::HashMap;
use std::fmt;

use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};

/// What the user changed about one message's trees: a node path and whether it is open.
pub type Expansion = HashMap<String, bool>;

/// One visible node of a tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
  /// How far below the root, which is depth 0.
  pub depth: usize,
  /// `data`, `data/stage`, `data/steps/0`; the root of a raw JSON payload is empty.
  pub path: String,
  /// The key or the index, when the node has one.
  pub name: Option<String>,
  pub kind: Kind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
  /// An object or an array: whether it is open, and its size as `{2}` or `[3]`.
  Branch { open: bool, size: String },
  /// A value, written as JSON.
  Leaf(String),
}

/// A JSON document with its object keys in the order they were written.
#[derive(Debug, Clone, PartialEq)]
pub enum Json {
  Null,
  Bool(bool),
  Number(serde_json::Number),
  String(String),
  Array(Vec<Json>),
  Object(Vec<(String, Json)>),
}

impl Json {
  /// Reads a document, or `None` when the text is not JSON.
  pub fn parse(text: &str) -> Option<Self> {
    serde_json::from_str(text).ok()
  }

  /// The value under `key`, when this is an object that has it.
  pub fn get(&self, key: &str) -> Option<&Json> {
    match self {
      Self::Object(entries) => entries
        .iter()
        .rev()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value),
      _ => None,
    }
  }

  /// The children of an object or an array, keyed by name or index.
  fn children(&self) -> Option<Vec<(String, &Json)>> {
    match self {
      Self::Object(entries) => Some(entries.iter().map(|(key, value)| (key.clone(), value)).collect()),
      Self::Array(items) => Some(
        items
          .iter()
          .enumerate()
          .map(|(index, value)| (index.to_string(), value))
          .collect(),
      ),
      _ => None,
    }
  }

  /// A leaf as JSON text, the way `JSON.stringify` writes it.
  fn leaf(&self) -> String {
    match self {
      Self::Null => "null".to_owned(),
      Self::Bool(value) => value.to_string(),
      Self::Number(number) => number.to_string(),
      Self::String(text) => serde_json::to_string(text).unwrap_or_default(),
      Self::Array(_) | Self::Object(_) => String::new(),
    }
  }
}

impl<'de> Deserialize<'de> for Json {
  fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
    struct JsonVisitor;

    impl<'de> Visitor<'de> for JsonVisitor {
      type Value = Json;

      fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a JSON value")
      }

      fn visit_unit<E: de::Error>(self) -> Result<Json, E> {
        Ok(Json::Null)
      }

      fn visit_bool<E: de::Error>(self, value: bool) -> Result<Json, E> {
        Ok(Json::Bool(value))
      }

      fn visit_i64<E: de::Error>(self, value: i64) -> Result<Json, E> {
        Ok(Json::Number(value.into()))
      }

      fn visit_u64<E: de::Error>(self, value: u64) -> Result<Json, E> {
        Ok(Json::Number(value.into()))
      }

      fn visit_f64<E: de::Error>(self, value: f64) -> Result<Json, E> {
        Ok(serde_json::Number::from_f64(value).map_or(Json::Null, Json::Number))
      }

      fn visit_str<E: de::Error>(self, value: &str) -> Result<Json, E> {
        Ok(Json::String(value.to_owned()))
      }

      fn visit_string<E: de::Error>(self, value: String) -> Result<Json, E> {
        Ok(Json::String(value))
      }

      fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Json, A::Error> {
        let mut items = Vec::new();
        while let Some(item) = sequence.next_element()? {
          items.push(item);
        }
        Ok(Json::Array(items))
      }

      fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Json, A::Error> {
        let mut entries = Vec::new();
        while let Some(entry) = map.next_entry()? {
          entries.push(entry);
        }
        Ok(Json::Object(entries))
      }
    }

    deserializer.deserialize_any(JsonVisitor)
  }
}

/// Whether a node is open: what the user chose, or the default of the GUI, which opens
/// only the top level.
pub fn is_open(expansion: Option<&Expansion>, path: &str, depth: usize) -> bool {
  expansion
    .and_then(|expansion| expansion.get(path).copied())
    .unwrap_or(depth < 1)
}

/// The nodes of `value` that are on screen, in reading order.
pub fn entries(value: &Json, name: Option<&str>, path: &str, expansion: Option<&Expansion>) -> Vec<Entry> {
  let mut out = Vec::new();
  walk(value, name, path, 0, expansion, &mut out);
  out
}

fn walk(
  value: &Json,
  name: Option<&str>,
  path: &str,
  depth: usize,
  expansion: Option<&Expansion>,
  out: &mut Vec<Entry>,
) {
  let Some(children) = value.children() else {
    out.push(Entry {
      depth,
      path: path.to_owned(),
      name: name.map(str::to_owned),
      kind: Kind::Leaf(value.leaf()),
    });
    return;
  };
  let open = is_open(expansion, path, depth);
  let size = if matches!(value, Json::Array(_)) {
    format!("[{}]", children.len())
  } else {
    format!("{{{}}}", children.len())
  };
  out.push(Entry {
    depth,
    path: path.to_owned(),
    name: name.map(str::to_owned),
    kind: Kind::Branch { open, size },
  });
  if open {
    for (key, child) in children {
      walk(child, Some(&key), &child_path(path, &key), depth + 1, expansion, out);
    }
  }
}

fn child_path(path: &str, key: &str) -> String {
  if path.is_empty() {
    key.to_owned()
  } else {
    format!("{path}/{key}")
  }
}

/// Every branch of `value`, open or not, with its depth.
pub fn branches(value: &Json, path: &str) -> Vec<(String, usize)> {
  fn collect(value: &Json, path: &str, depth: usize, out: &mut Vec<(String, usize)>) {
    let Some(children) = value.children() else {
      return;
    };
    out.push((path.to_owned(), depth));
    for (key, child) in children {
      collect(child, &child_path(path, &key), depth + 1, out);
    }
  }
  let mut out = Vec::new();
  collect(value, path, 0, &mut out);
  out
}

/// Opens every branch of `value`, or closes them all when every one is already open,
/// which is what `Space` on a message does to each of its trees.
pub fn toggle_all(expansion: &mut Expansion, value: &Json, path: &str) {
  let all = branches(value, path);
  let open = !all.iter().all(|(path, depth)| is_open(Some(expansion), path, *depth));
  for (path, _) in all {
    expansion.insert(path, open);
  }
}

/// Flips one node.
pub fn toggle(expansion: &mut Expansion, path: &str, depth: usize) {
  let open = is_open(Some(expansion), path, depth);
  expansion.insert(path.to_owned(), !open);
}

#[cfg(test)]
mod tests {
  use super::*;

  fn json(text: &str) -> Json {
    Json::parse(text).unwrap()
  }

  fn texts(entries: &[Entry]) -> Vec<String> {
    entries
      .iter()
      .map(|entry| {
        let name = entry
          .name
          .as_deref()
          .map(|name| format!("{name}: "))
          .unwrap_or_default();
        match &entry.kind {
          Kind::Branch { open, size } => format!(
            "{}{}{name}{size}",
            "  ".repeat(entry.depth),
            if *open { "-" } else { "+" }
          ),
          Kind::Leaf(json) => format!("{}{name}{json}", "  ".repeat(entry.depth)),
        }
      })
      .collect()
  }

  #[test]
  fn the_top_level_is_open_and_the_branches_below_it_closed() {
    let value = json(r#"{"stage": "deploy", "nested": {"deep": true}, "steps": ["a", "b", "c"], "n": 1.5, "x": null}"#);
    let shown = texts(&entries(&value, Some("data"), "data", None));
    assert_eq!(
      shown,
      [
        "-data: {5}",
        "  stage: \"deploy\"",
        "  +nested: {1}",
        "  +steps: [3]",
        "  n: 1.5",
        "  x: null"
      ],
      "in the order the keys were written"
    );
    assert_eq!(value.get("stage"), Some(&Json::String("deploy".to_owned())));
    assert!(Json::parse("not json").is_none());
  }

  #[test]
  fn one_node_toggles_and_space_opens_or_closes_every_branch() {
    let value = json(r#"{"nested": {"deep": {"deeper": 1}}}"#);
    let mut expansion = Expansion::new();
    toggle(&mut expansion, "nested", 1);
    assert_eq!(
      texts(&entries(&value, None, "", Some(&expansion))),
      ["-{1}", "  -nested: {1}", "    +deep: {1}"]
    );

    toggle_all(&mut expansion, &value, "");
    assert_eq!(
      entries(&value, None, "", Some(&expansion)).len(),
      4,
      "every branch is open"
    );
    toggle_all(&mut expansion, &value, "");
    assert_eq!(texts(&entries(&value, None, "", Some(&expansion))), ["+{1}"]);
  }
}
