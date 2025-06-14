//! he future for processing messages from a stream.
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
use xmtp_common::FutureWrapper;
use xmtp_db::{group_message::StoredGroupMessage, XmtpDb};
use xmtp_proto::xmtp::mls::api::v1::group_message;

/// Creates a future that processes a single message
pub trait Factory<'a> {
    /// Create a future
    fn create(&self, msg: group_message::V1) -> FutureWrapper<'a, Result<ProcessedMessage>>;
    /// Try to retrieve a message
    fn retrieve(&self, msg: &group_message::V1) -> Result<Option<StoredGroupMessage>>;
}

impl<'a, ApiClient, Db> Factory<'a> for ProcessMessage<ApiClient, Db>
where
    ApiClient: XmtpApi + 'a,
    Db: XmtpDb + 'a,
{
    fn create(&self, msg: group_message::V1) -> FutureWrapper<'a, Result<ProcessedMessage>> {
        let group_db = GroupDb::new(self.context.clone());
        let syncer = Syncer::new(self.context.clone());
        let processor = MessageProcessor::new(syncer, group_db);
        let future = processor.process(msg);

        FutureWrapper::new(future)
    }
    /// Try to retrieve a message
    fn retrieve(&self, msg: &group_message::V1) -> Result<Option<StoredGroupMessage>> {
        let db = GroupDb::new(self.context.clone());
        db.msg(None, msg).map_err(Into::into)
    }
}

/// Future that processes a group message from the network
pub struct ProcessMessage<ApiClient, Db> {
    context: Arc<XmtpMlsLocalContext<ApiClient, Db>>,
}

impl<ApiClient, Db> Clone for ProcessMessage<ApiClient, Db> {
    fn clone(&self) -> Self {
        Self {
            context: self.context.clone(),
        }
    }
}

// The processed message
pub struct ProcessedMessage {
    pub message: Option<StoredGroupMessage>,
    pub group_id: Vec<u8>,
    pub next_message: u64,
    pub tried_to_process: u64,
}

impl<ApiClient, Db> ProcessMessage<ApiClient, Db>
where
    ApiClient: XmtpApi,
    Db: XmtpDb,
{
    /// Creates a new `ProcessMessage` to handle processing of an MLS group message.
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
    /// * `Result<ProcessMessage<C>>` - A new future for processing the message, or an error if initialization fails
    ///
    /// # Example
    /// ```no_run
    /// let future = ProcessMessage::new(client, incoming_message)?;
    /// let processed = future.process().await?;
    /// ```
    pub fn new(context: Arc<XmtpMlsLocalContext<ApiClient, Db>>) -> ProcessMessage<ApiClient, Db> {
        Self { context }
    }
}

#[cfg(test)]
mod tests {
    use crate::groups::mls_sync::GroupMessageProcessingError;
    use crate::subscriptions::process_message::factory::MockGroupDatabase;
    use crate::subscriptions::process_message::factory::MockSync;
    use crate::subscriptions::SubscribeError;
    use crate::test::mock::generate_errored_summary;
    use crate::test::mock::generate_message_v1;
    use crate::test::mock::generate_messages_with_ids;
    use crate::test::mock::generate_stored_msg;
    use crate::test::mock::generate_successful_summary;

    use super::*;
    use rstest::*;
    use rstest_reuse::{self, *};

    #[template]
    #[rstest]
    #[case(vec![55, 60], vec![70, 80], 60)]
    #[case(vec![100], vec![84], 100)]
    #[case(vec![55, 89], vec![60, 70, 80, 84], 89)]
    #[case(vec![45, 55, 50, 80], vec![], 80)]
    #[case(vec![], vec![55, 60, 70], 55)]
    #[case(vec![35, 55, 75, 80, 85 ], vec![60, 70, 90, 100], 85)]
    #[case(vec![35, 55, 75, 80, 85 ], vec![60, 70], 85)]
    fn summary_cases(#[case] errors: Vec<u64>, #[case] success: Vec<u64>, #[case] expected: u64) {}

    #[rstest]
    #[xmtp_common::test]
    pub async fn test_process_returns_correct_cursor(
        #[values(5, 8, 10, 11, 13, 18)] current_message: u64,
    ) {
        let current_message = generate_message_v1(current_message);
        let mut mock_syncer = MockSync::new();
        let mut mock_db = MockGroupDatabase::new();
        mock_db.expect_last_cursor().times(1).returning(|_| Ok(3));
        mock_db.expect_msg().times(1).returning(|_, _| Ok(None));
        mock_syncer
            .expect_process()
            .times(1)
            // any error to get us to sync
            .returning(|_| {
                Err(SubscribeError::ReceiveGroup(Box::new(
                    GroupMessageProcessingError::InvalidPayload,
                )))
            });
        let messages = generate_messages_with_ids(&[4, 5, 8, 10, 11, 13, 18]);
        mock_syncer
            .expect_recover()
            .times(1)
            .returning(move |_| generate_successful_summary(&messages));
        let processed = MessageProcessor::new(mock_syncer, mock_db)
            .process(current_message.clone())
            .await;
        assert_eq!(processed.unwrap().next_message, current_message.id);
    }

    #[apply(summary_cases)]
    #[xmtp_common::test]
    pub async fn test_process_returns_correct_cursor_on_err(
        errors: Vec<u64>,
        success: Vec<u64>,
        expected: u64,
    ) {
        let current_message = generate_message_v1(*success.first().unwrap_or(&55));
        let mut mock_syncer = MockSync::new();
        let mut mock_db = MockGroupDatabase::new();
        // the last cursor is
        mock_db.expect_last_cursor().times(1).returning(|_| Ok(50));
        mock_db.expect_msg().times(1).returning(|_, _| Ok(None));
        mock_syncer
            .expect_process()
            .times(1)
            // any error to get us to sync
            .returning(|_| {
                Err(SubscribeError::ReceiveGroup(Box::new(
                    GroupMessageProcessingError::InvalidPayload,
                )))
            });
        mock_syncer
            .expect_recover()
            .times(1)
            .returning(move |_| generate_errored_summary(errors.as_slice(), success.as_slice()));
        let processed = MessageProcessor::new(mock_syncer, mock_db)
            .process(current_message)
            .await;
        assert_eq!(processed.unwrap().next_message, expected);
    }

    #[rstest]
    #[case(None)]
    #[case(Some(generate_stored_msg(55, xmtp_common::rand_vec::<32>())))]
    #[xmtp_common::test]
    pub async fn test_cursor_no_sync(#[case] message: Option<StoredGroupMessage>) {
        let current_message = generate_message_v1(55);
        let mock_syncer = MockSync::new();
        let mut mock_db = MockGroupDatabase::new();
        mock_db.expect_last_cursor().times(1).returning(|_| Ok(100));
        let mocked_m = message.clone();
        mock_db
            .expect_msg()
            .times(1)
            .returning(move |_, _| Ok(mocked_m.clone()));
        let processed = MessageProcessor::new(mock_syncer, mock_db)
            .process(current_message)
            .await;
        assert_eq!(processed.as_ref().unwrap().next_message, 55);
        if message.is_some() {
            assert!(processed.unwrap().message.is_some())
        }
    }
}
