use std::collections::HashMap;

use futures::{StreamExt, TryStreamExt, stream};
use xmtp_api::{ApiClientWrapper, GroupFilter, XmtpApi};
use xmtp_proto::types::{Cursor, GroupId};

use crate::subscriptions::SubscribeError;

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
    started_at: Cursor,
    /// last mesasage we got from the network
    /// If we get a message before this cursor, we should
    /// check if we synced after that cursor, and should
    /// prefer retrieving from the database
    last_streamed: Option<Cursor>,
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
    pub fn new(cursor: Cursor, started_at: Cursor) -> Self {
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
    pub(super) fn set(&mut self, cursor: Cursor) {
        self.last_streamed = Some(cursor);
    }

    /// Retrieves the current cursor position.
    ///
    /// Returns the cursor position or 0 if no cursor has been set yet.
    ///
    /// # Returns
    /// * `u64` - The current cursor position or 0 if unset
    pub(crate) fn last_streamed(&self) -> Cursor {
        self.last_streamed.unwrap_or(Default::default())
    }

    /// when did the stream start streaming for this group
    pub(crate) fn started(&self) -> Cursor {
        self.started_at
    }
}

impl std::fmt::Display for MessagePosition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.last_streamed())
    }
}

pub(super) trait Api {
    /// get the latest message for a cursor
    async fn query_latest_position(
        &self,
        group: &GroupId,
    ) -> Result<MessagePosition, SubscribeError>;
}

impl<A> Api for ApiClientWrapper<A>
where
    A: XmtpApi,
{
    async fn query_latest_position(
        &self,
        group: &GroupId,
    ) -> Result<MessagePosition, SubscribeError> {
        if let Some(msg) = self.query_latest_group_message(group).await? {
            Ok(MessagePosition::new(msg.cursor, msg.cursor))
        } else {
            Ok(MessagePosition::new(Default::default(), Default::default()))
        } // there is no cursor for this group yet
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct GroupList {
    list: HashMap<GroupId, MessagePosition>,
}

impl GroupList {
    pub(super) async fn new(list: Vec<GroupId>, api: &impl Api) -> Result<Self, SubscribeError> {
        let list = stream::iter(list)
            .map(|group| async {
                let position = api.query_latest_position(&group).await?;
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
                // TODO:d14n this is going to need to change
                // will not work with cursor from dif originators
                // i.e mixed commits & app msgs will screw up ordering
                GroupFilter::new(group_id.to_vec(), Some(cursor.last_streamed().sequence_id))
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

    pub(super) fn set(&mut self, group: impl AsRef<[u8]>, cursor: Cursor) {
        self.list
            .entry(group.as_ref().into())
            .and_modify(|c| c.set(cursor))
            .or_default();
    }
}
