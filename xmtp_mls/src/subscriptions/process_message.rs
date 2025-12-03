//! he future for processing messages from a stream.
//! When we receive a message from a stream, we treat it with special care.
//! Streams may receive messages out of order. Since we cannot rely on the order of messages
//! in a stream, we must defer to the 'sync' function whenever we receive a message that
//! depends on a previous message (like a commit).
//! The future for processing a single message from a stream

pub mod factory;

use super::Result;
use crate::context::XmtpSharedContext;
use crate::groups::summary::MessageIdentifierBuilder;
use factory::{GroupDatabase, GroupDb, MessageProcessor, Syncer};
use xmtp_common::BoxDynFuture;
use xmtp_db::group_message::StoredGroupMessage;
use xmtp_proto::types::Cursor;

/// Creates a future that processes sa single message
pub trait ProcessFutureFactory<'a> {
    fn create(
        &self,
        msg: xmtp_proto::types::GroupMessage,
    ) -> BoxDynFuture<'a, Result<ProcessedMessage>>;
    /// Try to retrieve a message
    fn retrieve(&self, msg: &xmtp_proto::types::GroupMessage)
    -> Result<Option<StoredGroupMessage>>;
}

impl<'a, Context> ProcessFutureFactory<'a> for ProcessMessageFuture<Context>
where
    Context: XmtpSharedContext + 'a,
{
    fn create(
        &self,
        msg: xmtp_proto::types::GroupMessage,
    ) -> BoxDynFuture<'a, Result<ProcessedMessage>> {
        let group_db = GroupDb::new(self.context.clone());
        let syncer = Syncer::new(self.context.clone());
        let processor = MessageProcessor::new(syncer, group_db);
        let future = processor.process(msg);

        Box::pin(future)
    }
    /// Try to retrieve a message
    fn retrieve(
        &self,
        msg: &xmtp_proto::types::GroupMessage,
    ) -> Result<Option<StoredGroupMessage>> {
        let db = GroupDb::new(self.context.clone());
        db.msg(None, msg).map_err(Into::into)
    }
}

/// Future that processes a group message from the network
pub struct ProcessMessageFuture<Context> {
    context: Context,
}

impl<Context: Clone> Clone for ProcessMessageFuture<Context> {
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
    pub next_message: Cursor,
    pub tried_to_process: Cursor,
}

impl<Context> ProcessMessageFuture<Context>
where
    Context: XmtpSharedContext,
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
    pub fn new(context: Context) -> ProcessMessageFuture<Context> {
        Self { context }
    }
}

#[cfg(test)]
mod tests {
    use crate::groups::mls_sync::GroupMessageProcessingError;
    use crate::subscriptions::SubscribeError;
    use crate::subscriptions::process_message::factory::MockGroupDatabase;
    use crate::subscriptions::process_message::factory::MockSync;
    use crate::test::mock::generate_message;
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
        use xmtp_common::rand_vec;

        let current_message = generate_message(current_message, &rand_vec::<16>());
        let mut mock_syncer = MockSync::new();
        let mut mock_db = MockGroupDatabase::new();
        let oid = current_message.originator_id();
        mock_db.expect_last_cursor().times(1).returning(move |_| {
            Ok(Cursor {
                sequence_id: 3,
                originator_id: oid,
            }
            .into())
        });
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
        assert_eq!(processed.unwrap().next_message, current_message.cursor);
    }

    #[apply(summary_cases)]
    #[xmtp_common::test]
    pub async fn test_process_returns_correct_cursor_on_err(
        errors: Vec<u64>,
        success: Vec<u64>,
        expected: u64,
    ) {
        use xmtp_common::Generate as _;
        use xmtp_proto::types::GroupId;

        use crate::test::mock::generate_errored_summary;

        let current_message =
            generate_message(*success.first().unwrap_or(&55), &GroupId::generate());
        let mut mock_syncer = MockSync::new();
        let mut mock_db = MockGroupDatabase::new();
        let oid = current_message.originator_id();
        // the last cursor is
        mock_db.expect_last_cursor().times(1).returning(move |_| {
            Ok(Cursor {
                sequence_id: 50,
                originator_id: oid,
            }
            .into())
        });
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
        assert_eq!(
            processed.unwrap().next_message,
            Cursor::v3_messages(expected)
        );
    }

    #[rstest]
    #[case(None)]
    #[case(Some(generate_stored_msg(Cursor::new(55, 0u32), xmtp_common::rand_vec::<32>())))]
    #[xmtp_common::test]
    pub async fn test_cursor_no_sync(#[case] message: Option<StoredGroupMessage>) {
        let current_message = generate_message(55, &[0]);
        let mock_syncer = MockSync::new();
        let mut mock_db = MockGroupDatabase::new();
        let oid = current_message.originator_id();
        mock_db.expect_last_cursor().times(1).returning(move |_| {
            Ok(Cursor {
                sequence_id: 100,
                originator_id: oid,
            }
            .into())
        });
        let mocked_m = message.clone();
        mock_db
            .expect_msg()
            .times(1)
            .returning(move |_, _| Ok(mocked_m.clone()));
        let processed = MessageProcessor::new(mock_syncer, mock_db)
            .process(current_message)
            .await;
        assert_eq!(
            processed.as_ref().unwrap().next_message,
            Cursor::v3_messages(55)
        );
        if message.is_some() {
            assert!(processed.unwrap().message.is_some())
        }
    }
}
