use crate::protocol::{CursorStore, CursorStoreError};
use parking_lot::Mutex;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::Arc;
use xmtp_proto::api::VectorClock;
use xmtp_proto::types::{Cursor, GlobalCursor, OriginatorId, OrphanedEnvelope, Topic};

#[derive(Clone)]
pub struct InMemoryCursorStore {
    topics: HashMap<Topic, GlobalCursor>,
    icebox: Arc<Mutex<HashSet<OrphanedEnvelope>>>,
    cutover_ns: Arc<Mutex<i64>>,
    last_checked_ns: Arc<Mutex<i64>>,
    migrated: Arc<Mutex<bool>>,
}

impl Default for InMemoryCursorStore {
    fn default() -> Self {
        Self {
            topics: HashMap::new(),
            icebox: Arc::new(Mutex::new(HashSet::new())),
            cutover_ns: Arc::new(Mutex::new(i64::MAX)),
            last_checked_ns: Arc::new(Mutex::new(0)),
            migrated: Arc::new(Mutex::new(false)),
        }
    }
}

impl InMemoryCursorStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record that a message for this topic with the given clock was received
    pub fn received(&mut self, topic: Topic, new_clock: &GlobalCursor) {
        let current = self.topics.entry(topic).or_default();
        current.merge(new_clock);
    }

    /// Get the current vector clock for this topic
    pub fn get_latest(&self, topic: &Topic) -> Option<&GlobalCursor> {
        self.topics.get(topic)
    }

    /// Compute the lowest common cursor across a set of topics.
    /// For each node_id, uses the **minimum** sequence ID seen across all topics.
    pub fn lowest_common_cursor(&self, topics: &[&Topic]) -> GlobalCursor {
        let mut min_clock = GlobalCursor::default();

        for topic in topics {
            if let Some(cursor) = self.get_latest(topic) {
                for (&node_id, &seq_id) in cursor {
                    min_clock
                        .entry(node_id)
                        .and_modify(|existing| *existing = (*existing).min(seq_id))
                        .or_insert(seq_id);
                }
            }
        }
        min_clock
    }

    /// Get the number of orphaned envelopes currently in the icebox
    #[cfg(test)]
    pub fn orphan_count(&self) -> usize {
        self.icebox.lock().len()
    }

    #[cfg(test)]
    pub fn icebox(&self) -> Vec<OrphanedEnvelope> {
        let icebox = self.icebox.lock();
        Vec::from_iter(icebox.clone())
    }
}

impl CursorStore for InMemoryCursorStore {
    fn lowest_common_cursor(
        &self,
        topics: &[&Topic],
    ) -> Result<xmtp_proto::types::GlobalCursor, crate::protocol::CursorStoreError> {
        Ok(self.lowest_common_cursor(topics))
    }

    fn lcc_maybe_missing(
        &self,
        topics: &[&Topic],
    ) -> Result<GlobalCursor, super::CursorStoreError> {
        Ok(self.lowest_common_cursor(topics))
    }

    fn latest(
        &self,
        topic: &xmtp_proto::types::Topic,
    ) -> Result<GlobalCursor, crate::protocol::CursorStoreError> {
        Ok(self
            .get_latest(topic)
            .cloned()
            .unwrap_or_else(GlobalCursor::default))
    }

    fn latest_per_originator(
        &self,
        topic: &xmtp_proto::types::Topic,
        originators: &[&OriginatorId],
    ) -> Result<GlobalCursor, crate::protocol::CursorStoreError> {
        Ok(self
            .get_latest(topic)
            .unwrap_or(&Default::default())
            .iter()
            .filter(|(k, _)| originators.contains(k))
            .map(|(&k, &v)| (k, v))
            .collect())
    }

    fn latest_for_topics(
        &self,
        topics: &mut dyn Iterator<Item = &Topic>,
    ) -> Result<HashMap<Topic, GlobalCursor>, super::CursorStoreError> {
        Ok(topics
            .map(|topic| (topic.clone(), self.latest(topic).unwrap_or_default()))
            .collect())
    }

    fn find_message_dependencies(
        &self,
        hash: &[&[u8]],
    ) -> Result<HashMap<Vec<u8>, Cursor>, super::CursorStoreError> {
        // in mem does not keep track of deps/commits
        Err(CursorStoreError::NoDependenciesFound(
            hash.iter().map(hex::encode).collect(),
        ))
    }

    fn ice(&self, orphans: Vec<OrphanedEnvelope>) -> Result<(), CursorStoreError> {
        let mut icebox = self.icebox.lock();
        (*icebox).extend(orphans);
        Ok(())
    }

    fn resolve_children(
        &self,
        cursors: &[Cursor],
    ) -> Result<Vec<OrphanedEnvelope>, CursorStoreError> {
        let icebox = self.icebox.lock();
        Ok(Vec::from_iter(resolve_children_inner(cursors, &icebox)))
    }

    fn set_cutover_ns(&self, cutover_ns: i64) -> Result<(), CursorStoreError> {
        *self.cutover_ns.lock() = cutover_ns;
        Ok(())
    }

    fn get_cutover_ns(&self) -> Result<i64, CursorStoreError> {
        Ok(*self.cutover_ns.lock())
    }

    fn has_migrated(&self) -> Result<bool, CursorStoreError> {
        Ok(*self.migrated.lock())
    }

    fn set_has_migrated(&self, has_migrated: bool) -> Result<(), CursorStoreError> {
        *self.migrated.lock() = has_migrated;
        Ok(())
    }

    fn get_last_checked_ns(&self) -> Result<i64, CursorStoreError> {
        Ok(*self.last_checked_ns.lock())
    }

    fn set_last_checked_ns(&self, last_checked_ns: i64) -> Result<(), CursorStoreError> {
        *self.last_checked_ns.lock() = last_checked_ns;
        Ok(())
    }
}

fn resolve_children_inner(
    cursors: &[Cursor],
    icebox: &HashSet<OrphanedEnvelope>,
) -> HashSet<OrphanedEnvelope> {
    let mut children: HashSet<OrphanedEnvelope> =
        cursors.iter().fold(HashSet::new(), |mut acc, cursor| {
            // extract if item in an icebox is child of the cursor
            let children = icebox
                .iter()
                .filter(|o| o.is_child_of(cursor))
                .cloned()
                .collect::<HashSet<_>>();
            acc.extend(children);
            acc
        });
    // recursively work through deps
    let cursors = children.iter().fold(Vec::new(), |mut acc, c| {
        if !c.depends_on.is_empty() {
            acc.push(c.cursor);
        }
        acc
    });
    if !cursors.is_empty() {
        let v = resolve_children_inner(&cursors, icebox);
        children.extend(v);
    }
    children
}

impl fmt::Debug for InMemoryCursorStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut entries = f.debug_map();

        for (topic, cursor) in &self.topics {
            // display topic as hex for readability
            let topic_hex = hex::encode(topic);
            entries.entry(&topic_hex, cursor);
        }

        entries.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cursor_with(kvs: &[(u32, u64)]) -> GlobalCursor {
        GlobalCursor::new(kvs.iter().cloned().collect())
    }

    #[xmtp_common::test]
    fn test_processed_and_get_latest() {
        let mut store = InMemoryCursorStore::new();
        let topic = topic("chat/abc");

        let cursor = cursor_with(&[(1, 10), (2, 5)]);
        store.received(topic.clone(), &cursor.clone());

        let latest = store.get_latest(&topic).unwrap();
        assert_eq!(latest.get(&1), 10);
        assert_eq!(latest.get(&2), 5);
    }

    #[xmtp_common::test]
    fn test_merge_on_processed() {
        let mut store = InMemoryCursorStore::new();
        let topic = topic("chat/merge");

        let c1 = cursor_with(&[(1, 10), (2, 5)]);
        let c2 = cursor_with(&[(1, 12), (2, 3), (3, 7)]);

        store.received(topic.clone(), &c1);
        store.received(topic.clone(), &c2);

        let latest = store.get_latest(&topic).unwrap();
        assert_eq!(latest.get(&1), 12);
        assert_eq!(latest.get(&2), 5);
        assert_eq!(latest.get(&3), 7);
    }

    #[xmtp_common::test]
    fn test_get_latest_nonexistent_topic() {
        let store = InMemoryCursorStore::new();
        let missing_topic = topic("does/not/exist");

        assert!(store.get_latest(&missing_topic).is_none());
    }

    #[xmtp_common::test]
    fn test_independent_topics() {
        let mut store = InMemoryCursorStore::new();

        let topic_a = topic("a");
        let topic_b = topic("b");

        store.received(topic_a.clone(), &cursor_with(&[(1, 1)]));
        store.received(topic_b.clone(), &cursor_with(&[(2, 2)]));

        let a = store.get_latest(&topic_a).unwrap();
        let b = store.get_latest(&topic_b).unwrap();

        assert_eq!(a.get(&1), 1);
        assert_eq!(b.get(&2), 2);
    }

    #[xmtp_common::test]
    fn test_merge_into_empty_store_creates_topic() {
        let mut store = InMemoryCursorStore::new();
        let topic = topic("new/topic");
        let cursor = cursor_with(&[(5, 9)]);

        store.received(topic.clone(), &cursor.clone());

        let stored = store.get_latest(&topic).unwrap();
        assert_eq!(stored.get(&5), 9);
    }

    fn topic(name: &str) -> Topic {
        Topic::from_bytes(name.as_bytes())
    }

    #[xmtp_common::test]
    fn test_lcc_normal_case() {
        let mut store = InMemoryCursorStore::new();

        store.received(topic("a"), &cursor_with(&[(1, 10), (2, 20)]));
        store.received(topic("b"), &cursor_with(&[(1, 15), (2, 12), (3, 9)]));
        store.received(topic("c"), &cursor_with(&[(1, 8), (3, 11)]));

        let lcc = store.lowest_common_cursor(&[&topic("a"), &topic("b"), &topic("c")]);

        assert_eq!(lcc.get(&1), 8); // min(10, 15, 8)
        assert_eq!(lcc.get(&2), 12); // min(20, 12)
        assert_eq!(lcc.get(&3), 9); // min(9, 11)
    }

    #[xmtp_common::test]
    fn test_lcc_with_missing_topic() {
        let mut store = InMemoryCursorStore::new();

        store.received(topic("a"), &cursor_with(&[(1, 10)]));
        store.received(topic("b"), &cursor_with(&[(1, 5)]));

        let lcc = store.lowest_common_cursor(&[&topic("a"), &topic("b"), &topic("not-found")]);

        assert_eq!(lcc.get(&1), 5); // min(10, 5)
    }

    #[xmtp_common::test]
    fn test_lcc_with_zero_values() {
        let mut store = InMemoryCursorStore::new();

        store.received(topic("x"), &cursor_with(&[(1, 0), (2, 4)]));
        store.received(topic("y"), &cursor_with(&[(1, 3), (2, 0)]));

        let lcc = store.lowest_common_cursor(&[&topic("x"), &topic("y")]);

        assert_eq!(lcc.get(&1), 0);
        assert_eq!(lcc.get(&2), 0);
    }

    #[xmtp_common::test]
    fn test_lcc_with_unseen_nodes() {
        let mut store = InMemoryCursorStore::new();

        store.received(topic("a"), &cursor_with(&[(1, 5)]));
        store.received(topic("b"), &cursor_with(&[(2, 7)]));

        let lcc = store.lowest_common_cursor(&[&topic("a"), &topic("b")]);

        assert_eq!(lcc.get(&1), 5);
        assert_eq!(lcc.get(&2), 7);
    }

    #[xmtp_common::test]
    fn test_lcc_with_no_cursors() {
        let store = InMemoryCursorStore::new();

        let result = store.lowest_common_cursor(&[&topic("a"), &topic("b")]);
        assert!(result.is_empty());
    }
}
