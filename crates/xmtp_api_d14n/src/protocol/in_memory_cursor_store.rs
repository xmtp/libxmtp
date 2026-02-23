use crate::protocol::{CursorStore, CursorStoreError};
use parking_lot::Mutex;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::Arc;
use xmtp_proto::api::VectorClock;
use xmtp_proto::types::{Cursor, GlobalCursor, OriginatorId, OrphanedEnvelope, Topic};

#[derive(Default, Clone)]
pub struct InMemoryCursorStore {
    topics: HashMap<Topic, GlobalCursor>,
    icebox: Arc<Mutex<HashSet<OrphanedEnvelope>>>,
}

impl InMemoryCursorStore {
    pub fn new() -> Self {
        Self {
            topics: HashMap::new(),
            icebox: Arc::new(Mutex::new(HashSet::new())),
        }
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
}
