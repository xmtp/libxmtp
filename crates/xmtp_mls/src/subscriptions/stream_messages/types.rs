use std::collections::HashSet;

use xmtp_proto::types::{Cursor, Topic, TopicCursor};

#[derive(thiserror::Error, Debug)]
pub enum MessageStreamError {
    #[error("received message for not subscribed group {id}", id = hex::encode(_0))]
    NotSubscribed(Vec<u8>),
    #[error("Invalid Payload")]
    InvalidPayload,
}

#[derive(Clone, Debug, Default)]
pub(super) struct GroupList {
    list: TopicCursor,
    // NOTE: if mem is a concern use a bloom filter
    // or create a garbage collection strategy
    seen: HashSet<Cursor>,
}

impl GroupList {
    pub(super) fn new(list: TopicCursor, seen: HashSet<Cursor>) -> Self {
        Self { list, seen }
    }

    pub(super) fn has_seen(&self, cursor: Cursor) -> bool {
        self.seen.contains(&cursor)
    }

    /// get the size of the group list
    #[allow(unused)]
    pub(super) fn len(&self) -> usize {
        self.list.len()
    }

    /// get all groups with their positions
    pub(super) fn groups_with_positions(&self) -> &TopicCursor {
        &self.list
    }

    /// get the `Cursor` for `group_id`, if any
    pub(super) fn position(&self, group_id: impl AsRef<[u8]>) -> Cursor {
        self.list
            .get(&Topic::new_group_message(group_id))
            .copied()
            .unwrap_or_default()
    }

    /// Check whether the group is already being tracked
    pub(super) fn contains(&self, group_id: impl AsRef<[u8]>) -> bool {
        self.list.contains_key(&Topic::new_group_message(group_id))
    }

    /// add a group at `Cursor` to this list
    pub(super) fn add(&mut self, group: impl AsRef<[u8]>, position: Cursor) {
        self.list.insert(Topic::new_group_message(group), position);
    }

    pub(super) fn set(&mut self, group: impl AsRef<[u8]>, cursor: Cursor) {
        self.list
            .entry(Topic::new_group_message(group))
            .and_modify(|g| *g = (*g).max(cursor))
            .or_insert(cursor);
        self.seen.insert(cursor);
    }
}
