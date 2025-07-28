use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex};
use xmtp_proto::xmtp::xmtpv4::envelopes::Cursor;

pub type SharedCursorStore = Arc<Mutex<CursorStore>>;

pub struct CursorStore {
    topics: HashMap<Vec<u8>, Cursor>,
}

impl CursorStore {
    pub fn new() -> Self {
        Self {
            topics: HashMap::new(),
        }
    }

    /// Record that a message for this topic with the given clock was processed
    pub fn processed(&mut self, topic: Vec<u8>, new_clock: &Cursor) {
        let current = self.topics.entry(topic).or_default();
        current.merge_cursor(new_clock);
    }

    /// Get the current vector clock for this topic
    pub fn get_latest(&self, topic: &[u8]) -> Option<&Cursor> {
        self.topics.get(topic)
    }

    /// Compute the lowest common cursor across a set of topics.
    /// For each node_id, uses the **minimum** sequence ID seen across all topics.
    pub fn lowest_common_cursor(&self, topics: &[Vec<u8>]) -> Option<Cursor> {
        let mut min_clock: HashMap<u32, u64> = HashMap::new();
        let mut seen_any = false;

        for topic in topics {
            if let Some(cursor) = self.get_latest(topic) {
                seen_any = true;
                for (&node_id, &seq_id) in &cursor.node_id_to_sequence_id {
                    min_clock
                        .entry(node_id)
                        .and_modify(|existing| *existing = (*existing).min(seq_id))
                        .or_insert(seq_id);
                }
            }
        }

        if seen_any {
            Some(Cursor {
                node_id_to_sequence_id: min_clock,
            })
        } else {
            None
        }
    }
}

impl fmt::Debug for CursorStore {
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

impl Default for CursorStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cursor_with(kvs: &[(u32, u64)]) -> Cursor {
        Cursor {
            node_id_to_sequence_id: kvs.iter().cloned().collect(),
        }
    }

    #[test]
    fn test_processed_and_get_latest() {
        let mut store = CursorStore::new();
        let topic = b"chat/abc".to_vec();

        let cursor = cursor_with(&[(1, 10), (2, 5)]);
        store.processed(topic.clone(), &cursor.clone());

        let latest = store.get_latest(&topic).unwrap();
        assert_eq!(latest.node_id_to_sequence_id.get(&1), Some(&10));
        assert_eq!(latest.node_id_to_sequence_id.get(&2), Some(&5));
    }

    #[test]
    fn test_merge_on_processed() {
        let mut store = CursorStore::new();
        let topic = b"chat/merge".to_vec();

        let c1 = cursor_with(&[(1, 10), (2, 5)]);
        let c2 = cursor_with(&[(1, 12), (2, 3), (3, 7)]);

        store.processed(topic.clone(), &c1);
        store.processed(topic.clone(), &c2);

        let latest = store.get_latest(&topic).unwrap();
        assert_eq!(latest.node_id_to_sequence_id.get(&1), Some(&12));
        assert_eq!(latest.node_id_to_sequence_id.get(&2), Some(&5));
        assert_eq!(latest.node_id_to_sequence_id.get(&3), Some(&7));
    }

    #[test]
    fn test_get_latest_nonexistent_topic() {
        let store = CursorStore::new();
        let missing_topic = b"does/not/exist".to_vec();

        assert!(store.get_latest(&missing_topic).is_none());
    }

    #[test]
    fn test_independent_topics() {
        let mut store = CursorStore::new();

        let topic_a = b"a".to_vec();
        let topic_b = b"b".to_vec();

        store.processed(topic_a.clone(), &cursor_with(&[(1, 1)]));
        store.processed(topic_b.clone(), &cursor_with(&[(2, 2)]));

        let a = store.get_latest(&topic_a).unwrap();
        let b = store.get_latest(&topic_b).unwrap();

        assert_eq!(a.node_id_to_sequence_id.get(&1), Some(&1));
        assert_eq!(b.node_id_to_sequence_id.get(&2), Some(&2));
    }

    #[test]
    fn test_merge_into_empty_store_creates_topic() {
        let mut store = CursorStore::new();
        let topic = b"new/topic".to_vec();
        let cursor = cursor_with(&[(5, 9)]);

        store.processed(topic.clone(), &cursor.clone());

        let stored = store.get_latest(&topic).unwrap();
        assert_eq!(stored.node_id_to_sequence_id.get(&5), Some(&9));
    }

    fn topic(name: &str) -> Vec<u8> {
        name.as_bytes().to_vec()
    }

    #[test]
    fn test_lcc_normal_case() {
        let mut store = CursorStore::new();

        store.processed(topic("a"), &cursor_with(&[(1, 10), (2, 20)]));
        store.processed(topic("b"), &cursor_with(&[(1, 15), (2, 12), (3, 9)]));
        store.processed(topic("c"), &cursor_with(&[(1, 8), (3, 11)]));

        let lcc = store
            .lowest_common_cursor(&[topic("a"), topic("b"), topic("c")])
            .unwrap();

        assert_eq!(lcc.node_id_to_sequence_id.get(&1), Some(&8));  // min(10, 15, 8)
        assert_eq!(lcc.node_id_to_sequence_id.get(&2), Some(&12)); // min(20, 12)
        assert_eq!(lcc.node_id_to_sequence_id.get(&3), Some(&9));  // min(9, 11)
    }

    #[test]
    fn test_lcc_with_missing_topic() {
        let mut store = CursorStore::new();

        store.processed(topic("a"), &cursor_with(&[(1, 10)]));
        store.processed(topic("b"), &cursor_with(&[(1, 5)]));

        let lcc = store
            .lowest_common_cursor(&[topic("a"), topic("b"), topic("not-found")])
            .unwrap();

        assert_eq!(lcc.node_id_to_sequence_id.get(&1), Some(&5));  // min(10, 5)
    }

    #[test]
    fn test_lcc_with_zero_values() {
        let mut store = CursorStore::new();

        store.processed(topic("x"), &cursor_with(&[(1, 0), (2, 4)]));
        store.processed(topic("y"), &cursor_with(&[(1, 3), (2, 0)]));

        let lcc = store
            .lowest_common_cursor(&[topic("x"), topic("y")])
            .unwrap();

        assert_eq!(lcc.node_id_to_sequence_id.get(&1), Some(&0));
        assert_eq!(lcc.node_id_to_sequence_id.get(&2), Some(&0));
    }

    #[test]
    fn test_lcc_with_unseen_nodes() {
        let mut store = CursorStore::new();

        store.processed(topic("a"), &cursor_with(&[(1, 5)]));
        store.processed(topic("b"), &cursor_with(&[(2, 7)]));

        let lcc = store
            .lowest_common_cursor(&[topic("a"), topic("b")])
            .unwrap();

        assert_eq!(lcc.node_id_to_sequence_id.get(&1), Some(&5));
        assert_eq!(lcc.node_id_to_sequence_id.get(&2), Some(&7));
    }

    #[test]
    fn test_lcc_with_no_cursors() {
        let store = CursorStore::new();

        let result = store.lowest_common_cursor(&[topic("a"), topic("b")]);
        assert!(result.is_none());
    }
}