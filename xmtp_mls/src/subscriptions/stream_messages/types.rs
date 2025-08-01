use std::collections::HashMap;

use futures::{stream, StreamExt, TryStreamExt};
use xmtp_api::{ApiClientWrapper, GroupFilter, XmtpApi};
use xmtp_common::types::GroupId;
use xmtp_db::prelude::QueryGroupMessage;

use crate::subscriptions::SubscribeError;

use super::extract_message_cursor;

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
    pub fn new(cursor: u64, started_at: u64) -> Self {
        Self {
            last_streamed: Some(cursor),
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

pub(super) trait Api {
    /// existing method: pure network
    async fn query_latest_position<C>(
        &self,
        group: &GroupId,
        db: &impl QueryGroupMessage<C>,
    ) -> Result<MessagePosition, SubscribeError>
    where
        C: xmtp_db::ConnectionExt;
}

impl<A> Api for ApiClientWrapper<A>
where
    A: XmtpApi,
{
    async fn query_latest_position<C>(
        &self,
        group: &GroupId,
        db: &impl QueryGroupMessage<C>,
    ) -> Result<MessagePosition, SubscribeError>
    where
        C: xmtp_db::ConnectionExt,
    {
        // Try from DB
        if let Ok(Some(cursor)) = db.get_latest_sequence_id_for_group(group) {
            tracing::debug!(
                "Using local DB sequence_id {} for group {:?}",
                cursor,
                hex::encode(group)
            );
            return Ok(MessagePosition::new(cursor as u64, cursor as u64));
        }

        // Fallback to network
        tracing::debug!("Falling back to network for group {:?}", hex::encode(group));

        if let Some(msg) = self.query_latest_group_message(group).await? {
            let cursor = extract_message_cursor(&msg).ok_or(MessageStreamError::InvalidPayload)?;
            Ok(MessagePosition::new(cursor, cursor))
        } else {
            Ok(MessagePosition::new(0, 0)) // nothing yet
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct GroupList {
    list: HashMap<GroupId, MessagePosition>,
}

impl GroupList {
    pub(super) async fn new<C>(
        list: Vec<GroupId>,
        api: &impl Api,
        db: &impl QueryGroupMessage<C>,
    ) -> Result<Self, SubscribeError>
    where
        C: xmtp_db::ConnectionExt,
    {
        let list = stream::iter(list)
            .map(|group| async {
                let position = api.query_latest_position(&group, db).await?;
                Ok((group, position))
            })
            .buffer_unordered(8)
            .try_fold(HashMap::new(), async move |mut map, (group, position)| {
                map.insert(group, position);
                Ok::<_, SubscribeError>(map)
            })
            .await?;
        Ok(Self { list })
    }

    pub(super) fn filters(&self) -> Vec<GroupFilter> {
        self.list
            .iter()
            .map(|(group_id, cursor)| {
                GroupFilter::new(group_id.to_vec(), Some(cursor.last_streamed()))
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
