use std::collections::{HashMap, HashSet};

use xmtp_proto::types::{Cursor, GroupId, OriginatorId, SequenceId};

#[derive(thiserror::Error, Debug)]
pub enum MessageStreamError {
    #[error("received message for not subscribed group {id}", id = hex::encode(_0))]
    NotSubscribed(Vec<u8>),
    #[error("Invalid Payload")]
    InvalidPayload,
}

/// the position of this message in the backend topic
/// based only upon messages from the stream
#[derive(Default, Clone, PartialEq, Eq)]
pub struct MessagePosition {
    /// last mesasage we got from the network
    /// If we get a message before this cursor, we should
    /// check if we synced after that cursor, and should
    /// prefer retrieving from the database
    last_streamed: HashMap<OriginatorId, SequenceId>,
}

impl std::fmt::Debug for MessagePosition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MessagePosition")
            .field("last_streamed", &self.last_streamed)
            .finish()
    }
}

impl MessagePosition {
    pub fn new(cursor: Cursor) -> Self {
        let mut last_map = HashMap::new();
        last_map.insert(cursor.originator_id, cursor.sequence_id);
        Self {
            last_streamed: last_map,
        }
    }
    /// Updates the cursor position for this message.
    ///
    /// Sets the cursor to a specific position in the message stream, which
    /// helps track which messages have been processed.
    ///
    /// # Arguments
    /// * `cursor` - The new cursor position to set
    pub(super) fn set(&mut self, cursor: Cursor) {
        self.last_streamed
            .insert(cursor.originator_id, cursor.sequence_id);
    }

    /// Retrieves the current cursor position.
    ///
    /// Returns the cursor position or 0 if no cursor has been set yet.
    ///
    /// # Returns
    /// * `u64` - The current cursor position or 0 if unset
    pub(crate) fn last_streamed(&self) -> HashMap<u32, u64> {
        self.last_streamed.clone()
    }
}

impl std::fmt::Display for MessagePosition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.last_streamed())
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct GroupList {
    list: HashMap<GroupId, MessagePosition>,
    // NOTE: if mem is a concern use a bloom filter
    // or create a garbage collection strategy
    seen: HashSet<Cursor>,
}

impl GroupList {
    pub(super) fn new(list: Vec<GroupId>, seen: HashSet<Cursor>) -> Self {
        Self {
            list: list.into_iter().map(|g| (g, Default::default())).collect(),
            seen,
        }
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
    pub(super) fn groups_with_positions(&self) -> Vec<(GroupId, MessagePosition)> {
        self.list
            .iter()
            .map(|(id, pos)| (id.clone(), pos.clone()))
            .collect()
    }

    /// get the `MessagePosition` for `group_id`, if any
    pub(super) fn position(&self, group_id: impl AsRef<[u8]>) -> Option<MessagePosition> {
        self.list.get(group_id.as_ref()).cloned()
    }

    /// Check whether the group is already being tracked
    pub(super) fn contains(&self, group_id: impl AsRef<[u8]>) -> bool {
        self.list.contains_key(group_id.as_ref())
    }

    /// add a group at `MessagePosition` to this list
    pub(super) fn add(&mut self, group: impl AsRef<[u8]>, position: MessagePosition) {
        self.list.insert(group.as_ref().to_vec().into(), position);
    }

    pub(super) fn set(&mut self, group: impl AsRef<[u8]>, cursor: Cursor) {
        self.list
            .entry(group.as_ref().into())
            .and_modify(|c| c.set(cursor))
            .or_insert_with(|| {
                let mut pos = MessagePosition::default();
                pos.set(cursor);
                pos
            });
        self.seen.insert(cursor);
    }
}
