//! The future for processing messages from a stream.
//! When we receive a message from a stream, we treat it with special care.
//! Streams may receive messages out of order. Since we cannot rely on the order of messages
//! in a stream, we must defer to the 'sync' function whenever we receive a message that
//! depends on a previous message (like a commit).

use super::{Result, SubscribeError};
use crate::groups::{scoped_client::ScopedGroupClient, summary::SyncSummary, MlsGroup};
use xmtp_common::{retry_async, Retry};
use xmtp_db::{group_message::StoredGroupMessage, refresh_state::EntityKind, StorageError};
use xmtp_id::InboxIdRef;
use xmtp_proto::xmtp::mls::api::v1::group_message;

/// Future that processes a group message from the network
pub struct ProcessMessageFuture<Client> {
    client: Client,
    msg: group_message::V1,
}

// The processed message
pub struct ProcessedMessage {
    pub message: Option<StoredGroupMessage>,
    pub group_id: Vec<u8>,
    pub next_message: u64,
}

impl<C> ProcessMessageFuture<C>
where
    C: ScopedGroupClient,
{
    /// Creates a new `ProcessMessageFuture` to handle processing of an MLS group message.
    ///
    /// This function initializes the future with the client and message that needs processing.
    /// It's the entry point for handling messages received from a stream.
    ///
    /// # Arguments
    /// * `client` - A client implementing the `ScopedGroupClient` trait that provides context and access to group operations
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
    pub fn new(client: C, msg: group_message::V1) -> Result<ProcessMessageFuture<C>> {
        Ok(Self { client, msg })
    }

    /// Returns the inbox ID associated with the client processing this message.
    ///
    /// This is a helper method that provides access to the inbox identifier,
    /// which is useful for logging and debugging purposes.
    ///
    /// # Returns
    /// * `InboxIdRef<'_>` - A reference to the inbox ID
    fn inbox_id(&self) -> InboxIdRef<'_> {
        self.client.inbox_id()
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
    #[tracing::instrument(skip_all)]
    pub(crate) async fn process(self) -> Result<ProcessedMessage> {
        let group_message::V1 {
            // the cursor ID is the position in the monolithic backend topic
            id: ref cursor_id,
            ref created_ns,
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
            SyncSummary::default()
        };

        let conn = self.client.context_ref().db();
        // Load the message from the DB to handle cases where it may have been already processed in
        // another thread
        let new_message =
            conn.get_group_message_by_timestamp(&self.msg.group_id, *created_ns as i64)?;

        if let Some(msg) = new_message {
            tracing::debug!(
                "[{}] processed stream envelope [{}]",
                self.inbox_id(),
                &cursor_id
            );
            Ok(ProcessedMessage {
                message: Some(msg),
                next_message: *cursor_id,
                group_id: self.msg.group_id,
            })
        } else {
            tracing::warn!(
                cursor_id,
                inbox_id = self.inbox_id(),
                group_id = hex::encode(&self.msg.group_id),
                "no further processing for streamed message [{}] in group [{}]",
                &cursor_id,
                hex::encode(&self.msg.group_id),
            );
            let mut processed = summary.process;
            processed.new_messages.sort_by_key(|k| k.cursor);
            let next: Option<u64> = processed.new_messages.first().map(|f| f.cursor);
            // if we have no new messages, set the cursor to the latest total message we processed
            // or, if we did not process anything, set it back to 0.
            let next = next.unwrap_or(processed.last().unwrap_or(0));
            Ok(ProcessedMessage {
                message: None,
                next_message: next,
                group_id: self.msg.group_id,
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
        let process_result = retry_async!(
            Retry::default(),
            (async {
                let (group, _) = MlsGroup::new_validated(&self.client, self.msg.group_id.clone())?;
                let epoch = group.epoch().await?;

                tracing::debug!(
                    inbox_id = self.inbox_id(),
                    group_id = hex::encode(&self.msg.group_id),
                    cursor_id = self.msg.id,
                    epoch = epoch,
                    "epoch={} for [{}] in apply_or_sync_with_message()",
                    epoch,
                    self.inbox_id(),
                );
                group
                    .process_message(&self.msg, false)
                    .await
                    // NOTE: We want to make sure we retry an error in process_message
                    .map_err(SubscribeError::ReceiveGroup)
            })
        );

        match process_result {
            Err(SubscribeError::ReceiveGroup(e)) => {
                tracing::warn!("error processing streamed message {e}");
                self.attempt_message_recovery().await
            }
            // This should never occur because we map the error to `ReceiveGroup`
            // But still exists defensively
            Err(e) => {
                tracing::error!(
                    inbox_id = self.client.inbox_id(),
                    group_id = hex::encode(&self.msg.group_id),
                    cursor_id = self.msg.id,
                    err = e.to_string(),
                    "unexpected error in process stream entry {}",
                    e
                );
                SyncSummary::default()
            }
            Ok(Some(msg)) => {
                tracing::trace!(
                    cursor_id = self.msg.id,
                    inbox_id = self.inbox_id(),
                    group_id = hex::encode(&self.msg.group_id),
                    "message process in stream success"
                );
                // we didnt need to sync
                SyncSummary::single(msg)
            }
            Ok(None) => {
                // nothing processed
                tracing::trace!(
                    cursor_id = self.msg.id,
                    inbox_id = self.inbox_id(),
                    group_id = hex::encode(&self.msg.group_id),
                    "message process in stream success"
                );
                // we didnt need to sync
                SyncSummary::default()
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
        let check_for_last_cursor = || -> std::result::Result<i64, StorageError> {
            self.client
                .context_ref()
                .db()
                .get_last_cursor_for_id(&self.msg.group_id, EntityKind::Group)
        };

        let last_synced_id = check_for_last_cursor()?;
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
    async fn attempt_message_recovery(&self) -> SyncSummary {
        let group = MlsGroup::new(
            &self.client,
            self.msg.group_id.clone(),
            None,
            self.msg.created_ns as i64,
        );
        let epoch = group.epoch().await.unwrap_or(0);
        tracing::debug!(
            inbox_id = self.client.inbox_id(),
            group_id = hex::encode(&self.msg.group_id),
            cursor_id = self.msg.id,
            epoch = epoch,
            "attempting recovery sync for group {} in epoch {}",
            xmtp_common::fmt::truncate_hex(hex::encode(&self.msg.group_id)),
            epoch
        );
        // Swallow errors here, since another process may have successfully saved the message
        // to the DB
        let sync = group.sync_with_conn().await;
        if let Err(summary) = sync {
            tracing::warn!(
                inbox_id = self.client.inbox_id(),
                group_id = hex::encode(&self.msg.group_id),
                cursor_id = self.msg.id,
                "recovery sync triggered by streamed message failed",
            );
            tracing::warn!("{summary}");
            summary
        } else {
            let epoch = group.epoch().await.unwrap_or(0);
            let summary = sync.expect("checked for error");
            tracing::debug!(
                inbox_id = self.client.inbox_id(),
                group_id = hex::encode(&self.msg.group_id),
                cursor_id = self.msg.id,
                "recovery sync triggered by streamed message successful, epoch = {} for group = {}",
                epoch,
                xmtp_common::fmt::truncate_hex(hex::encode(&self.msg.group_id))
            );
            tracing::debug!("{summary}");
            summary
        }
    }
}

//TODO: Would be GREAT to unit test this module with a mock client
