use std::collections::HashMap;

use xmtp_api::GroupFilter;
use xmtp_common::types::GroupId;
use xmtp_db::{prelude::QueryRefreshState, refresh_state::EntityKind};

use crate::{context::XmtpSharedContext, subscriptions::SubscribeError};

#[derive(thiserror::Error, Debug)]
pub enum MessageStreamError {
    #[error("received message for not subscribed group {id}", id = hex::encode(_0))]
    NotSubscribed(Vec<u8>),
    #[error("Invalid Payload")]
    InvalidPayload,
}

/// the position of this message in the backend topic
/// based only upon messages from the stream
#[derive(Copy, Default, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MessagePosition {
    started_at: u64,
    /// last mesasage we got from the network
    /// If we get a message before this cursor, we should
    /// check if we synced after that cursor, and should
    /// prefer retrieving from the database
    last_streamed: Option<u64>,
}

impl std::fmt::Debug for MessagePosition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MessagePosition")
            .field("started_at", &self.started_at)
            .field("last_streamed", &self.last_streamed)
            .finish()
    }
}

impl MessagePosition {
    pub fn new(last_streamed: u64, started_at: u64) -> Self {
        Self {
            last_streamed: Some(last_streamed),
            started_at,
        }
    }
    /// Updates the cursor position for this message.
    ///
    /// Sets the cursor to a specific position in the message stream, which
    /// helps track which messages have been processed.
    ///
    /// # Arguments
    /// * `cursor` - The new cursor position to set
    pub(super) fn set(&mut self, cursor: u64) {
        self.last_streamed = Some(cursor);
    }

    /// Retrieves the current cursor position.
    ///
    /// Returns the cursor position or 0 if no cursor has been set yet.
    ///
    /// # Returns
    /// * `u64` - The current cursor position or 0 if unset
    pub(crate) fn last_streamed(&self) -> u64 {
        self.last_streamed.unwrap_or(0)
    }

    /// when did the stream start streaming for this group
    pub(crate) fn started(&self) -> u64 {
        self.started_at
    }
}

impl std::fmt::Display for MessagePosition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.last_streamed())
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

            let message_position = MessagePosition::new(last_streamed, db_cursor);
            group_list.insert(group_id, message_position);
        }

        Ok(Self { list: group_list })
    }

    pub(super) fn filters(&self) -> Vec<GroupFilter> {
        self.list
            .iter()
            .map(|(group_id, cursor)| {
                let filter_cursor = std::cmp::max(cursor.last_streamed(), cursor.started());
                GroupFilter::new(group_id.to_vec(), Some(filter_cursor))
            })
            .collect()
    }

    /// get the `MessagePosition` for `group_id`, if any
    pub(super) fn position(&self, group_id: impl AsRef<[u8]>) -> Option<MessagePosition> {
        self.list.get(group_id.as_ref()).copied()
    }

    /// Check whether the group is already being tracked
    pub(super) fn contains(&self, group_id: impl AsRef<[u8]>) -> bool {
        self.list.contains_key(group_id.as_ref())
    }

    /// add a group at `MessagePosition` to this list
    pub(super) fn add(&mut self, group: impl AsRef<[u8]>, position: MessagePosition) {
        self.list.insert(group.as_ref().to_vec().into(), position);
    }

    pub(super) fn set(&mut self, group: impl AsRef<[u8]>, cursor: u64) {
        self.list
            .entry(group.as_ref().into())
            .and_modify(|c| c.set(cursor))
            .or_default();
    }
}
