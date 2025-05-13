//! The future for processing messages from a stream.
//! When we receive a message from a stream, we treat it with special care.
//! Streams may receive messages out of order. Since we cannot rely on the order of messages
//! in a stream, we must defer to the 'sync' function whenever we receive a message that
//! depends on a previous message (like a commit).
//! The future for processing a single message from a stream
use super::{Result, SubscribeError};
use crate::{
    groups::{
        mls_sync::GroupMessageProcessingError,
        summary::{MessageIdentifierBuilder, SyncSummary},
        MlsGroup,
    },
    intents::ProcessIntentError,
};
use xmtp_common::{retry_async, Retry};
use xmtp_db::{group_message::StoredGroupMessage, refresh_state::EntityKind, XmtpDb};
use xmtp_api::XmtpApi;
use xmtp_id::InboxIdRef;
use xmtp_proto::xmtp::mls::api::v1::group_message;
use std::sync::Arc;
use crate::context::XmtpMlsLocalContext;

/// Future that processes a group message from the network
pub struct ProcessMessageFuture<ApiClient, Db> {
    context: Arc<XmtpMlsLocalContext<ApiClient, Db>>,
    msg: group_message::V1,
}

// The processed message
pub struct ProcessedMessage {
    pub message: Option<StoredGroupMessage>,
    pub group_id: Vec<u8>,
    pub next_message: Option<u64>,
    pub tried_to_process: u64,
}

impl<ApiClient, Db> ProcessMessageFuture<ApiClient, Db>
where
    ApiClient: XmtpApi,
    Db: XmtpDb,
{
    /// Creates a new `ProcessMessageFuture` to handle processing of an MLS group message.
    ///
    /// This function initializes the future with the client and message that needs processing.
    /// It's the entry point for handling messages received from a stream.
    ///
    /// # Arguments
    /// * `context` - The `XmtpMlsLocalContext` provides context and access to the network and
    ///   database
    /// * `msg` - The group message to be processed (V1 version)
    ///
    /// # Returns
    /// * `Result<ProcessMessageFuture<C>>` - A new future for processing the message, or an error if initialization fails
    ///
    /// # Example
    /// ```no_run
    /// let future = ProcessMessageFuture::new(client, incoming_message)?;
    /// let processed = future.process().await?;
    /// ```
    pub fn new(
        context: Arc<XmtpMlsLocalContext<ApiClient, Db>>,
        msg: group_message::V1,
    ) -> Result<ProcessMessageFuture<ApiClient, Db>> {
        Ok(Self { context, msg })
    }

    /// Returns the inbox ID associated with the client processing this message.
    ///
    /// This is a helper method that provides access to the inbox identifier,
    /// which is useful for logging and debugging purposes.
    ///
    /// # Returns
    /// * `InboxIdRef<'_>` - A reference to the inbox ID
    fn inbox_id(&self) -> InboxIdRef<'_> {
        self.context.inbox_id()
    }

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
    #[tracing::instrument(skip_all, level = "debug")]
    pub(crate) async fn process(self) -> Result<ProcessedMessage> {
        let group_message::V1 {
            // the cursor ID is the position in the monolithic backend topic
            id: ref cursor_id,
            ..
        } = self.msg;

        tracing::debug!(
            inbox_id = self.inbox_id(),
            group_id = hex::encode(&self.msg.group_id),
            cursor_id,
            "[{}]  is about to process streamed envelope for group {} cursor_id=[{}]",
            self.inbox_id(),
            xmtp_common::fmt::truncate_hex(hex::encode(&self.msg.group_id)),
            &cursor_id
        );

        let summary = if self.needs_to_sync(*cursor_id)? {
            self.apply_or_sync_with_message().await
        } else {
            SyncSummary::single(MessageIdentifierBuilder::from(&self.msg).build()?)
        };
        let conn = self.context.db();
        // Load the message from the DB to handle cases where it may have been already processed in
        // another thread
        // if we can't get it directly by ID, get by teimstamp and group_id
        let new_message = summary.new_message_by_id(self.msg.id)
            .and_then(|m| {
                if m.group_id != self.msg.group_id {
                    tracing::warn!("new message with cursor from sync is not equivalent to message from network with same cursor.");
                    return None;
                }
                tracing::trace!("attempting to get message by id {:?}", m.internal_id);
                m.internal_id.clone()
            }).map(|id| {
                conn.get_group_message(id)
            })
            .inspect(|m| {
                if let Ok(Some(_)) = m {
                    tracing::trace!("successfully retrieved message by id ")
                } else {
                    tracing::trace!("trying to get message by timestamp")
                }
            })
            .unwrap_or(conn.get_group_message_by_timestamp(&self.msg.group_id, self.msg.created_ns as i64))
            .inspect(|m| {
                if m.is_some() {
                    tracing::trace!("retrieved message by timestamp")
                } else {
                    tracing::trace!("no message found with group_id and timestamp")
                }
            })?;
        if let Some(msg) = new_message {
            tracing::debug!(
                "[{}] processed stream envelope @cursor=[{}], future is resolved",
                self.inbox_id(),
                &cursor_id
            );
            Ok(ProcessedMessage {
                message: Some(msg),
                next_message: Some(*cursor_id),
                group_id: self.msg.group_id,
                tried_to_process: self.msg.id,
            })
        } else {
            let processed = summary
                .process
                .new_messages
                .iter()
                .find(|m| m.cursor == *cursor_id);
            tracing::warn!(
                cursor_id,
                inbox_id = self.inbox_id(),
                group_id = hex::encode(&self.msg.group_id),
                "no message present in db for message @cursor=[{}] in group [{}] of maybe_kind [{:?}] and maybe commit taking group to epoch@[{:?}]",
                &cursor_id,
                hex::encode(&self.msg.group_id),
                processed.as_ref().and_then(|m| m.intent_kind),
                processed.as_ref().and_then(|m| m.group_context.as_ref().map(|g| g.epoch()))
            );
            let next: Option<u64> = summary.process.first_new().or(summary.process.first());
            Ok(ProcessedMessage {
                message: None,
                next_message: next,
                group_id: self.msg.group_id,
                tried_to_process: self.msg.id,
            })
        }
    }

    /// Applies a message by processing it, or by calling out to sync.
    ///
    /// This function handles the actual processing of a message from the stream:
    /// 1. Creates or validates the MLS group
    /// 2. Attempts to process the message
    /// 3. If processing fails for any reason (incl. epoch change), triggers message recovery
    ///    "Message Recovery" indicates a sync process that tries to 'recover' the message which
    ///    failed to process.
    ///
    /// The function includes retry logic to handle transient failures.
    ///
    /// # Returns
    /// * `SyncSummary` - A summary of the synchronization process, including information
    ///   about any new messages or processing errors
    ///
    /// # Note
    /// This function is designed to be resilient to failures, with built-in retry
    /// mechanisms and fallback to recovery synchronization when needed.
    async fn apply_or_sync_with_message(&self) -> SyncSummary {
        use SubscribeError::*;
        let process_result = retry_async!(
            Retry::default(),
            (async {
                let (group, _) = MlsGroup::new_cached(self.context.clone(), &self.msg.group_id)?;
                let epoch = group.epoch().await?;

                tracing::debug!(
                    inbox_id = self.inbox_id(),
                    group_id = hex::encode(&self.msg.group_id),
                    cursor_id = self.msg.id,
                    epoch = epoch,
                    "epoch={} for [{}] in process_stream_entry()",
                    epoch,
                    self.inbox_id(),
                );
                group
                    .process_message(&self.msg, false)
                    .instrument(tracing::debug_span!("process_message"))
                    .await
                    // NOTE: We want to make sure we retry an error in process_message
                    .map_err(SubscribeError::ReceiveGroup)
            })
        );

        match process_result {
            Err(ReceiveGroup(GroupMessageProcessingError::MessageAlreadyProcessed(msg)))
            | Err(ReceiveGroup(GroupMessageProcessingError::ProcessIntent(
                ProcessIntentError::MessageAlreadyProcessed(msg),
            ))) => {
                tracing::debug!("message {msg:?} already processed");
                SyncSummary::single(msg)
            }
            Err(ReceiveGroup(e)) => self.attempt_message_recovery(e).await,
            Err(e) => {
                // This should never occur because we map the error to `ReceiveGroup`
                // But still exists defensively
                tracing::error!(
                    inbox_id = self.context.inbox_id(),
                    group_id = hex::encode(&self.msg.group_id),
                    cursor_id = self.msg.id,
                    err = e.to_string(),
                    "process stream entry {:?}",
                    e
                );
                SyncSummary::default()
            }

            Ok(msg) => {
                tracing::trace!(
                    cursor_id = self.msg.id,
                    inbox_id = self.inbox_id(),
                    group_id = hex::encode(&self.msg.group_id),
                    "message process in stream success, synced single msg @cursor={},group_id={}",
                    msg.cursor,
                    xmtp_common::fmt::truncate_hex(hex::encode(&msg.group_id))
                );
                SyncSummary::single(msg)
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
    fn needs_to_sync(&self, current_msg_cursor: u64) -> Result<bool> {
        let last_synced_id = self
            .context
            .db()
            .get_last_cursor_for_id(&self.msg.group_id, EntityKind::Group)?;
        if last_synced_id < current_msg_cursor as i64 {
            tracing::debug!(
                "stream does require sync; last_synced@[{}], this message @[{}]",
                last_synced_id,
                current_msg_cursor
            );
        } else {
            tracing::debug!(
                "stream does not require sync; last_synced@[{}], this message @[{}]",
                last_synced_id,
                current_msg_cursor
            );
        }
        Ok(last_synced_id < current_msg_cursor as i64)
    }

    /// Attempts to recover from a failed message processing by performing a sync.
    ///
    /// When regular message processing fails, this function attempts a full group
    /// synchronization to recover the message and bring the local state in line
    /// with the network state.
    ///
    /// The function creates a new MLS group instance and triggers a synchronization,
    /// handling any errors that may occur during the recovery process.
    ///
    /// # Returns
    /// * `SyncSummary` - A summary of the recovery synchronization process
    ///
    /// # Note
    /// This function gracefully handles synchronization failures, as another process
    /// may have already successfully processed the message.
    async fn attempt_message_recovery(&self, e: impl std::error::Error) -> SyncSummary {
        let group = MlsGroup::new(
            self.context.clone(),
            self.msg.group_id.clone(),
            None,
            self.msg.created_ns as i64,
        );
        let epoch = group.epoch().await.unwrap_or(0);
        tracing::debug!(
            inbox_id = self.context.inbox_id(),
            group_id = hex::encode(&self.msg.group_id),
            cursor_id = self.msg.id,
            epoch = epoch,
            "processing streamed message @cursor=[{}] failed with [{e}], attempting recovery sync for group {} in epoch {}",
            self.msg.id,
            xmtp_common::fmt::debug_hex(&self.msg.group_id),
            epoch
        );
        // Swallow errors here, since another process may have successfully saved the message
        // to the DB
        let sync = group.sync_with_conn().await;
        match sync {
            Ok(summary) => {
                let epoch = group.epoch().await.unwrap_or(0);
                tracing::debug!(
                    inbox_id = self.context.inbox_id(),
                    group_id = hex::encode(&self.msg.group_id),
                    cursor_id = self.msg.id,
                    "recovery sync processed=[{}] messages, group@[{}] now in epoch=[{}] with the first decryptable message @cursor=[{:?}]",
                    summary.process.total(),
                    xmtp_common::fmt::truncate_hex(hex::encode(&self.msg.group_id)),
                    epoch,
                    summary.process.first_new()
                );
                tracing::debug!("{summary}");
                summary
            }
            Err(summary) => {
                tracing::warn!(
                    inbox_id = self.context.inbox_id(),
                    group_id = hex::encode(&self.msg.group_id),
                    cursor_id = self.msg.id,
                    "recovery sync triggered by streamed message failed",
                );
                tracing::warn!("{summary}");
                summary
            }
        }
    }
}
