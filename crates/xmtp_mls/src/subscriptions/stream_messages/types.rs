use std::collections::HashSet;

use xmtp_proto::types::{Cursor, GlobalCursor, Topic, TopicCursor};

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
    pub(super) fn len(&self) -> usize {
        self.list.len()
    }

    /// get all groups with their positions
    pub(super) fn groups_with_positions(&self) -> &TopicCursor {
        &self.list
    }

    /// get the `GlobalCursor` for `group_id`, if any
    pub(super) fn position(&self, group_id: impl AsRef<[u8]>) -> GlobalCursor {
        self.list.get_group(group_id).clone()
    }

    /// Check whether the group is already being tracked
    pub(super) fn contains(&self, group_id: impl AsRef<[u8]>) -> bool {
        self.list.contains_group(group_id.as_ref())
    }

    /// add a group at `GlobalCursor` to this list
    pub(super) fn add(&mut self, group: impl AsRef<[u8]>, position: GlobalCursor) {
        self.list.insert(Topic::new_group_message(group), position);
    }

    pub(super) fn set(&mut self, group: impl AsRef<[u8]>, cursor: Cursor) {
        self.list
            .group_entry(group)
            .and_modify(|g| g.apply(&cursor))
            .or_insert(GlobalCursor::from(cursor));
        self.seen.insert(cursor);
    }
}
