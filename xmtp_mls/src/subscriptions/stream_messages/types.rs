use std::collections::HashMap;

use xmtp_api::GroupFilter;
use xmtp_configuration::Originators;
use xmtp_db::{prelude::QueryRefreshState, refresh_state::EntityKind};
use xmtp_proto::types::{Cursor, GroupId};

use crate::{context::XmtpSharedContext, subscriptions::SubscribeError};

#[derive(thiserror::Error, Debug)]
pub enum MessageStreamError {
    #[error("received message for not subscribed group {id}", id = hex::encode(_0))]
    NotSubscribed(Vec<u8>),
    #[error("Invalid Payload")]
    InvalidPayload,
}

/// the position of this group in the backend topic
/// based only upon messages from the stream
#[derive(Default, Clone, PartialEq, Eq)]
pub struct MessagePosition {
    last_synced: HashMap<u32, u64>,
    /// last mesasage we got from the network
    /// If we get a message before this cursor, we should
    /// check if we synced after that cursor, and should
    /// prefer retrieving from the database
    /// TODO:d14n this is a global cursor (better Display impl)
    last_streamed: HashMap<u32, u64>,
}

impl std::fmt::Debug for MessagePosition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MessagePosition")
            .field("last_synced", &self.last_synced)
            .field("last_streamed", &self.last_streamed)
            .finish()
    }
}

impl MessagePosition {
    pub fn new(last_synced_cursor: Cursor, last_streamed_cursor: Cursor) -> Self {
        let mut synced_map = HashMap::new();
        synced_map.insert(
            last_synced_cursor.originator_id,
            last_synced_cursor.sequence_id,
        );
        let mut last_streamed_map = HashMap::new();
        last_streamed_map.insert(
            last_streamed_cursor.originator_id,
            last_streamed_cursor.sequence_id,
        );
        Self {
            last_streamed: last_streamed_map,
            last_synced: synced_map,
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

    // if our last streamed is greater than this cursor, we have already seen the item
    pub fn has_seen(&self, cursor: Cursor) -> bool {
        let sid = self
            .last_streamed
            .get(&cursor.originator_id)
            .copied()
            .unwrap_or(0);
        sid > cursor.sequence_id
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

    pub(crate) fn synced(&self) -> HashMap<u32, u64> {
        self.last_synced.clone()
    }

    // /// stream started after this cursor
    // pub(crate) fn synced_after(&self, cursor: Cursor) -> bool {
    //     let sid = self
    //         .last_synced
    //         .get(&cursor.originator_id)
    //         .copied()
    //         .unwrap_or(0);
    //     sid > cursor.sequence_id
    // }

    // /// last sync before the stream started was before this cursor
    // pub(crate) fn synced_before(&self, cursor: Cursor) -> bool {
    //     let sid = self
    //         .last_synced
    //         .get(&cursor.originator_id)
    //         .copied()
    //         .unwrap_or(0);
    //     sid < cursor.sequence_id
    // }
}

impl std::fmt::Display for MessagePosition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.last_streamed())
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct GroupList {
    list: HashMap<GroupId, MessagePosition>,
}

impl GroupList {
    pub(super) fn new(
        list: Vec<GroupId>,
        ctx: &impl XmtpSharedContext,
    ) -> Result<Self, SubscribeError> {
        let db = ctx.db();
        let mut existing_positions = db.get_last_cursor_for_ids(&list, EntityKind::Group)?;

        let mut group_list = HashMap::new();

        for group_id in list {
            let db_cursor = existing_positions.remove(group_id.as_ref()).unwrap_or(0) as u64;

            // Query shared last_streamed mapping
            let last_streamed = ctx
                .get_shared_last_streamed(group_id.as_ref())
                .unwrap_or(db_cursor); // fallback to db cursor if not found

            let message_position = MessagePosition::new(
                // TODO:(nm) This will 100% break with decentralization
                Cursor::v3_messages(db_cursor),     // last_streamed
                Cursor::v3_messages(last_streamed), // started
            );
            group_list.insert(group_id, message_position);
        }

        Ok(Self { list: group_list })
    }

    pub(super) fn filters(&self) -> Vec<GroupFilter> {
        self.list
            .iter()
            .map(|(group_id, cursor)| {
                let map = cursor.synced();
                let sid = map
                    .get(&(Originators::MLS_COMMITS as u32))
                    .copied()
                    .unwrap_or(0);
                let sid2 = map
                    .get(&(Originators::APPLICATION_MESSAGES as u32))
                    .copied()
                    .unwrap_or(0);
                // TODO:d14n this is going to need to change
                // will not work with cursor from dif originators
                // i.e mixed commits & app msgs will screw up ordering
                // the cursor store PR selects the right cursor to start from w/o us
                // having to choose
                GroupFilter::new(group_id.to_vec(), Some(std::cmp::max(sid, sid2)))
            })
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
    }
}
