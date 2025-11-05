//! Stream message processor that uses Syning to handle out of order messages

use super::ProcessedMessage;
use crate::context::XmtpSharedContext;
use crate::groups::{
    MlsGroup,
    mls_sync::GroupMessageProcessingError,
    summary::{MessageIdentifier, SyncSummary},
};
use crate::subscriptions::SubscribeError;
use crate::subscriptions::process_message::MessageIdentifierBuilder;
use tracing::Instrument;
use xmtp_common::{Retry, retry_async};
use xmtp_db::group::ConversationType;
use xmtp_db::prelude::*;
use xmtp_db::{StorageError, group_message::StoredGroupMessage, refresh_state::EntityKind};
use xmtp_proto::types::Cursor;

#[cfg_attr(test, mockall::automock)]
pub trait GroupDatabase {
    /// Get the last cursor for a message
    fn last_cursor(&self, group_id: &[u8], originator_id: u32) -> Result<Cursor, StorageError>;
    /// get a message from the database
    // not needless, required by mockall
    #[allow(clippy::needless_lifetimes)]
    fn msg<'a>(
        &self,
        id: Option<&'a MessageIdentifier>,
        msg: &xmtp_proto::types::GroupMessage,
    ) -> Result<Option<StoredGroupMessage>, StorageError>;
}

#[derive(Clone)]
pub struct GroupDb<Context>(Context);

impl<Context> GroupDb<Context> {
    pub fn new(context: Context) -> Self {
        Self(context)
    }
}

impl<Context> GroupDatabase for GroupDb<Context>
where
    Context: XmtpSharedContext,
{
    fn last_cursor(&self, group_id: &[u8], originator_id: u32) -> Result<Cursor, StorageError> {
        self.0.db().get_last_cursor_for_originator(
            group_id,
            EntityKind::ApplicationMessage,
            originator_id,
        )
    }

    fn msg(
        &self,
        id: Option<&MessageIdentifier>,
        msg: &xmtp_proto::types::GroupMessage,
    ) -> Result<Option<StoredGroupMessage>, StorageError> {
        let conn = self.0.db();
        id.and_then(|m| {
            if m.group_id != msg.group_id {
                return None;
            }
            m.internal_id.clone()
        })
        .map(|id| conn.get_group_message(id))
        .unwrap_or_else(|| conn.get_group_message_by_timestamp(&msg.group_id, msg.timestamp()))
        .map_err(StorageError::from)
    }
}

#[cfg_attr(test, mockall::automock)]
pub trait Sync {
    /// Try to process a single mesage
    async fn process(
        &self,
        msg: &xmtp_proto::types::GroupMessage,
    ) -> Result<MessageIdentifier, SubscribeError>;
    /// Try to recover from failing to process a message
    async fn recover(&self, msg: &xmtp_proto::types::GroupMessage) -> SyncSummary;
}

#[derive(Clone)]
pub struct Syncer<Context>(Context);
impl<Context> Syncer<Context> {
    pub fn new(context: Context) -> Self {
        Self(context)
    }
}

impl<Context> Sync for Syncer<Context>
where
    Context: XmtpSharedContext,
{
    async fn process(
        &self,
        msg: &xmtp_proto::types::GroupMessage,
    ) -> Result<MessageIdentifier, SubscribeError> {
        let (group, _) = MlsGroup::new_cached(self.0.clone(), &msg.group_id)?;
        let epoch = group.epoch().await?;
        tracing::debug!(
            "client@[{}] about to process streamed message @cursor=[{}] for group @epoch=[{}]",
            xmtp_common::fmt::truncate_hex(self.0.inbox_id()),
            msg.cursor,
            epoch,
        );

        group
            .process_message(msg, false)
            .instrument(tracing::debug_span!("process_message"))
            .await
            .map_err(|e| SubscribeError::ReceiveGroup(Box::new(e)))
    }

    async fn recover(&self, msg: &xmtp_proto::types::GroupMessage) -> SyncSummary {
        let group = MlsGroup::new(
            self.0.clone(),
            msg.group_id.to_vec(),
            None,
            ConversationType::Group,
            msg.timestamp(),
        );
        match group.sync_with_conn().await {
            Ok(summary) => {
                let epoch = group.epoch().await.unwrap_or(0);
                tracing::debug!(
                    "recovery sync processed=[{}] messages, group@[{}] now in epoch=[{}] with the first decryptable message @cursor=[{:?}]",
                    summary.process.total(),
                    xmtp_common::fmt::truncate_hex(hex::encode(&msg.group_id)),
                    epoch,
                    summary.process.first_new()
                );
                tracing::debug!("{summary}");
                summary
            }
            Err(summary) => {
                tracing::warn!(
                    inbox_id = self.0.inbox_id(),
                    group_id = hex::encode(&msg.group_id),
                    cursor_id = %msg.cursor,
                    "recovery sync triggered by streamed message failed",
                );
                tracing::warn!("{summary}");
                summary
            }
        }
    }
}

#[derive(Clone)]
pub struct MessageProcessor<S, D> {
    syncer: S,
    group_db: D,
}

impl<S, D> MessageProcessor<S, D> {
    pub fn new(syncer: S, group_db: D) -> Self {
        Self { syncer, group_db }
    }
}

impl<S, D> MessageProcessor<S, D>
where
    S: Sync,
    D: GroupDatabase,
{
    /// Processes a group message received from a stream.
    ///
    /// This is the main entry point for the future. It handles the complete lifecycle
    /// of message processing:
    /// 1. Checks if synchronization is needed
    /// 2. Processes the stream entry if needed
    /// 3. Retrieves the processed message from the database
    /// 4. Returns metadata about the processing result
    ///    - All messages are sorted by cursor.
    ///    - If no message was able to be processed in the sync, 'processed_message' will be 'None'.
    ///    - if the current message processed succesfully, the cursor is set to that message
    ///    - if multiple messages were succesfully processed, but the current message failed to
    ///    process, 'next_message' is set to the cursor of the next sucessfully processed message.
    ///    - if no messages were sucessfully processed, the cursor is set to the latest message which
    ///    failed to process.
    ///
    /// The function handles the complexities of out-of-order message delivery and
    /// ensures proper synchronization when necessary.
    ///
    /// # Returns
    /// * `Result<ProcessedMessage>` - Information about the processed message including:
    ///   - The stored message (if available)
    ///   - The group ID
    ///   - The next message cursor position
    ///
    /// # Errors
    /// Returns an error if any step in the processing pipeline fails.
    ///
    /// # Tracing
    /// This function includes tracing instrumentation to aid in debugging and monitoring.
    #[tracing::instrument(skip_all, level = "trace")]
    pub(crate) async fn process(
        self,
        msg: xmtp_proto::types::GroupMessage,
    ) -> Result<ProcessedMessage, SubscribeError> {
        let summary = if self.needs_to_sync(&msg)? {
            self.process_or_recover(&msg).await
        } else {
            // if we dont need to sync, the message should be in the database
            SyncSummary::single(MessageIdentifierBuilder::from(&msg).build()?)
        };

        let new_message = self
            .group_db
            .msg(summary.new_message_by_id(msg.cursor), &msg)?;

        if let Some(new_msg) = new_message {
            Ok(ProcessedMessage {
                message: Some(new_msg.clone()),
                next_message: msg.cursor,
                group_id: new_msg.group_id.clone(),
                tried_to_process: msg.cursor,
            })
        } else {
            let next: Cursor = summary.process.last_errored().unwrap_or(msg.cursor);
            Ok(ProcessedMessage {
                message: None,
                next_message: next,
                group_id: msg.group_id.to_vec(),
                tried_to_process: msg.cursor,
            })
        }
    }

    async fn process_or_recover(&self, msg: &xmtp_proto::types::GroupMessage) -> SyncSummary {
        use SubscribeError::*;
        // try to process the message with retries
        let process_result =
            retry_async!(Retry::default(), (async { self.syncer.process(msg).await }));

        match process_result {
            // if it failed try recovery
            Err(ReceiveGroup(m)) => {
                if matches!(
                    *m,
                    GroupMessageProcessingError::MessageAlreadyProcessed(_)
                        | GroupMessageProcessingError::ProcessIntent(_)
                ) {
                    return SyncSummary::single(msg.into());
                }
                self.syncer.recover(msg).await
            }
            Err(e) => {
                // This should never occur because we map the error to `ReceiveGroup`
                // But still exists defensively
                tracing::error!(
                    group_id = hex::encode(&msg.group_id),
                    cursor_id = %msg.cursor,
                    err = e.to_string(),
                    "process stream entry {:?}",
                    e
                );
                SyncSummary::default()
            }
            Ok(processed_msg) => {
                tracing::trace!(
                    cursor_id = %msg.cursor,
                    group_id = hex::encode(&msg.group_id),
                    "message process in stream success, synced single msg @cursor={},group_id={}",
                    processed_msg.cursor,
                    xmtp_common::fmt::truncate_hex(hex::encode(&processed_msg.group_id))
                );
                SyncSummary::single(processed_msg)
            }
        }
    }

    /// Determines if a sync is needed for a message.
    ///
    /// This function checks if the current message cursor is ahead of the last
    /// synchronized cursor for the group that we keep in the local database.
    /// If the message cursor is greater than
    /// the last synchronized cursor, synchronization is required.
    ///
    /// This is essential for handling out-of-order message delivery, as it ensures
    /// that messages are properly synchronized before processing.
    ///
    /// # Arguments
    /// * `current_msg_cursor` - The cursor position of the current message
    ///
    /// # Returns
    /// * `Result<bool>` - `true` if synchronization is needed, `false` otherwise
    ///
    /// # Errors
    /// Returns an error if the database query for the last cursor fails.
    fn needs_to_sync(&self, msg: &xmtp_proto::types::GroupMessage) -> Result<bool, SubscribeError> {
        let last_synced_id = self
            .group_db
            .last_cursor(&msg.group_id, msg.originator_id())?;
        // _*NOTE:*_ this should never happen since we pass the originator to the db query
        // but exists defensively regardless.
        if msg.originator_id() != last_synced_id.originator_id {
            return Err(SubscribeError::MismatchedOriginators {
                expected: msg.originator_id(),
                got: last_synced_id.originator_id,
            });
        }
        if last_synced_id.sequence_id < msg.sequence_id() {
            tracing::debug!(
                "stream requires sync; last_synced@[{}], this message @[{}]",
                last_synced_id,
                msg.cursor
            );
        } else {
            tracing::debug!(
                "stream does not require sync; last_synced@[{}], this message @[{}]",
                last_synced_id,
                msg.cursor
            );
        }
        Ok(last_synced_id.sequence_id < msg.cursor.sequence_id)
    }
}
