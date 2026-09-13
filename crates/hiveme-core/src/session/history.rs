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

//! What the session makes of the stored history: the topic tree and the pruning pass.
//!
//! The rows themselves, paging, marking read, and clearing are [`crate::storage`]; the
//! session only reshapes what it answers.

use std::collections::BTreeMap;

use crate::config::History;
use crate::storage::{Store, TopicRow};

use super::types::TopicNode;

/// Builds the topic tree by splitting every stored topic on `/`.
///
/// A node exists for every segment, so `hiveme/build/ci` puts `build` in the tree even
/// though nothing was ever published to it. Every nonempty path can be selected to
/// show its descendants. Counts are rolled up, so a collapsed branch shows that
/// something below it is unread.
pub fn build_tree(topics: &[TopicRow]) -> Vec<TopicNode> {
  #[derive(Default)]
  struct Node {
    children: BTreeMap<String, Node>,
    unread: u32,
    messages: u32,
  }

  fn insert(node: &mut Node, segments: &[&str], row: &TopicRow) {
    node.unread = node.unread.saturating_add(row.unread);
    node.messages = node.messages.saturating_add(row.messages);
    if let Some((head, rest)) = segments.split_first() {
      insert(node.children.entry((*head).to_owned()).or_default(), rest, row);
    }
  }

  fn convert(path: &str, label: &str, node: &Node) -> TopicNode {
    TopicNode {
      id: path.to_owned(),
      label: label.to_owned(),
      topic: (!path.is_empty()).then(|| path.to_owned()),
      unread: node.unread,
      messages: node.messages,
      children: node
        .children
        .iter()
        .map(|(segment, child)| convert(&format!("{path}/{segment}"), segment, child))
        .collect(),
    }
  }

  let mut root = Node::default();
  for row in topics {
    let segments: Vec<&str> = row.topic.split('/').collect();
    insert(&mut root, &segments, row);
  }
  root
    .children
    .iter()
    .map(|(segment, child)| convert(segment, segment, child))
    .collect()
}

/// One pruning pass with `gui.history`, logged the way the loop has always logged it.
pub fn prune(store: &Store, history: &History) {
  match store.prune(history) {
    Ok(pruned) if pruned.total() > 0 => log::info!(
      "pruned {} message(s): {} over the per-topic cap, {} past the retention window",
      pruned.total(),
      pruned.by_count,
      pruned.by_age
    ),
    Ok(_) => log::debug!("nothing to prune"),
    Err(error) => log::warn!("the history could not be pruned: {error}"),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn row(topic: &str, unread: u32, messages: u32) -> TopicRow {
    TopicRow {
      topic: topic.to_owned(),
      first_seen_ts: "2026-09-12T09:41:23.512Z".to_owned(),
      last_seen_ts: "2026-09-12T09:41:23.512Z".to_owned(),
      unread,
      messages,
    }
  }

  #[test]
  fn the_tree_follows_the_topic_hierarchy() {
    let tree = build_tree(&[row("hiveme/info", 2, 5), row("hiveme/build/ci", 1, 3)]);

    assert_eq!(tree.len(), 1);
    let root = &tree[0];
    assert_eq!(root.id, "hiveme");
    assert_eq!(root.label, "hiveme");
    assert_eq!(
      root.topic.as_deref(),
      Some("hiveme"),
      "a parent without direct messages is selectable"
    );
    assert_eq!(root.unread, 3, "unread rolls up into the parent");
    assert_eq!(root.messages, 8);

    let labels: Vec<&str> = root.children.iter().map(|child| child.label.as_str()).collect();
    assert_eq!(labels, ["build", "info"], "children are in alphabetical order");

    let build = &root.children[0];
    assert_eq!(build.id, "hiveme/build");
    assert_eq!(
      build.topic.as_deref(),
      Some("hiveme/build"),
      "an intermediate topic selects its subtree"
    );
    assert_eq!(build.children[0].id, "hiveme/build/ci");
    assert_eq!(build.children[0].topic.as_deref(), Some("hiveme/build/ci"));
  }

  #[test]
  fn a_topic_that_is_also_a_parent_is_selectable_and_still_has_children() {
    let tree = build_tree(&[row("hiveme", 1, 1), row("hiveme/info", 0, 2)]);

    let root = &tree[0];
    assert_eq!(root.topic.as_deref(), Some("hiveme"));
    assert_eq!(root.messages, 3);
    assert_eq!(root.children.len(), 1);
  }

  #[test]
  fn a_topic_outside_the_prefix_is_its_own_root() {
    let tree = build_tree(&[row("hiveme/info", 0, 1), row("$SYS/broker/uptime", 0, 1)]);

    let roots: Vec<&str> = tree.iter().map(|node| node.id.as_str()).collect();
    assert_eq!(roots, ["$SYS", "hiveme"]);
  }

  #[test]
  fn an_empty_leading_segment_is_a_group_without_a_topic() {
    let tree = build_tree(&[row("/leading", 1, 1)]);

    assert_eq!(tree[0].label, "");
    assert_eq!(tree[0].topic, None);
    assert_eq!(tree[0].children[0].topic.as_deref(), Some("/leading"));
  }

  #[test]
  fn an_empty_store_has_an_empty_tree() {
    assert!(build_tree(&[]).is_empty());
  }
}
