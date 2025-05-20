//! The future for processing messages from a stream.
//! When we receive a message from a stream, we treat it with special care.
//! Streams may receive messages out of order. Since we cannot rely on the order of messages
//! in a stream, we must defer to the 'sync' function whenever we receive a message that
//! depends on a previous message (like a commit).
//! The future for processing a single message from a stream

pub mod factory;

use super::Result;
use crate::context::XmtpMlsLocalContext;
use crate::groups::summary::MessageIdentifierBuilder;
use factory::{GroupDatabase, GroupDb, MessageProcessor, Syncer};
use std::sync::Arc;
use xmtp_api::XmtpApi;
use xmtp_db::{group_message::StoredGroupMessage, XmtpDb};
use xmtp_proto::xmtp::mls::api::v1::group_message;

/// Future that processes a group message from the network
#[cfg_attr(test, faux::create)]
pub struct ProcessMessageFuture<ApiClient, Db> {
    msg: group_message::V1,
    processor: MessageProcessor<Syncer<ApiClient, Db>, GroupDb<ApiClient, Db>>,
}

// The processed message
pub struct ProcessedMessage {
    pub message: Option<StoredGroupMessage>,
    pub group_id: Vec<u8>,
    pub next_message: Option<u64>,
    pub tried_to_process: u64,
}

#[cfg_attr(test, faux::methods)]
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
    ) -> ProcessMessageFuture<ApiClient, Db> {
        let group_db = GroupDb::new(context.clone());
        let syncer = Syncer::new(context.clone());
        let processor = MessageProcessor::new(syncer, group_db);
        Self { msg, processor }
    }

    pub async fn process(self) -> Result<ProcessedMessage> {
        self.processor.process(&self.msg).await
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use xmtp_api::test_utils::MockApiClient;
    use xmtp_db::MockXmtpDb;
    use xmtp_db::NotFound;

    use crate::groups::mls_sync::GroupMessageProcessingError;
    use crate::groups::summary::SyncSummary;
    use crate::subscriptions::process_message::factory::MockGroupDatabase;
    use crate::subscriptions::process_message::factory::MockSync;
    use crate::subscriptions::SubscribeError;
    use crate::test::mock::generate_messages_with_ids;
    use crate::{
        groups::summary::ProcessSummary,
        test::mock::{generate_message_v1, generate_mock_context},
    };

    use super::*;

    #[xmtp_common::test]
    pub async fn test_process_returns_correct_cursor() {
        xmtp_common::logger();
        let (tx, _) = tokio::sync::broadcast::channel(32);
        let mock_context = generate_mock_context(tx);
        let current_message = generate_message_v1(5);
        let messages = generate_messages_with_ids(vec![4, 5, 8, 10, 11, 13, 18]);
        let mut mock_syncer = MockSync::new();
        let mut mock_db = MockGroupDatabase::new();
        // the last cursor is
        mock_db.expect_last_cursor().times(1).returning(|_| Ok(3));
        mock_db.expect_msg().times(1).returning(|_, _| Ok(None));
        mock_syncer
            .expect_process()
            .times(1)
            // any error to get us to sync
            .returning(|_| {
                Err(SubscribeError::ReceiveGroup(
                    GroupMessageProcessingError::InvalidPayload,
                ))
            });
        let mocked = messages.clone();
        mock_syncer
            .expect_recover()
            .times(1)
            .returning(move |_| SyncSummary {
                publish_errors: vec![],
                process: ProcessSummary {
                    total_messages: HashSet::from_iter(mocked.iter().map(|m| m.id)),
                    new_messages: mocked.iter().map(Into::into).collect(),
                    errored: Vec::new(),
                },
                post_commit_errors: vec![],
                other: None,
            });
        let processed = MessageProcessor::new(mock_syncer, mock_db)
            .process(&current_message)
            .await;
        assert_eq!(processed.unwrap().next_message, Some(4));
    }

    pub async fn test_process_returns_correct_cursor_no_sync() {
        todo!()
    }
}
