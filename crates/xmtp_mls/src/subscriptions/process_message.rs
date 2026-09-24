//! The future for processing messages from a stream.
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
use xmtp_proto::types::{Cursor, GroupId};

/// Creates a future that processes a single message
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
    pub group_id: GroupId,
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

/// Outcome of running a single raw group message through the processing pipeline,
/// independent of any stream/poll machinery.
///
/// Dedup (skipping already-seen cursors) and cursor bookkeeping are intentionally
/// **not** handled here — they are the caller's responsibility, because different
/// consumers track them differently. The live [`super::stream_messages`] stream uses
/// a per-stream `GroupList`; the XIP-83 bidi multiplexing manager keeps central
/// per-topic high-water marks plus per-subscriber in-flight cursors. This seam is
/// only the decode half (DB fast-path, then decrypt/store + recovery-sync), so both
/// consumers decode identically.
#[derive(Debug)]
pub struct Processed {
    /// The decrypted message to deliver, if this envelope produced one. `None` means
    /// the envelope was processed but surfaced nothing (e.g. a commit); the caller
    /// should still advance its cursor to `next_cursor`.
    pub message: Option<StoredGroupMessage>,
    /// The group this envelope belongs to.
    pub group_id: GroupId,
    /// The cursor the caller should advance this group to after handling the result.
    pub next_cursor: Cursor,
    /// The cursor of the envelope we attempted to process (for tracking/logging).
    pub tried: Cursor,
}

/// The result of [`prepare`]: the synchronous pre-step of the pipeline. Either the
/// message was already stored locally (fast path) or it needs the async decrypt/store
/// pass run on it.
pub(crate) enum Prepared {
    /// Fast-path hit — no async work needed. Carries the stored message by value so
    /// consumers never face a "hit without a message" state.
    Ready {
        message: StoredGroupMessage,
        group_id: GroupId,
        cursor: Cursor,
    },
    /// Needs the async `create()` pass; carries the message to run it on.
    NeedsProcessing(xmtp_proto::types::GroupMessage),
}

/// Synchronous pre-step shared by the live stream and the bidi manager: probe the DB
/// fast path. Returns the stored message on a hit (no decrypt), or the message to run
/// the async pass on.
pub(crate) fn prepare<'a>(
    factory: &impl ProcessFutureFactory<'a>,
    msg: xmtp_proto::types::GroupMessage,
) -> Result<Prepared> {
    // Fast path: already stored locally — no decrypt, advance to the message's cursor.
    if let Some(stored) = factory.retrieve(&msg)? {
        return Ok(Prepared::Ready {
            message: stored,
            group_id: msg.group_id,
            cursor: msg.cursor,
        });
    }
    Ok(Prepared::NeedsProcessing(msg))
}

/// Synchronous post-step shared by the live stream and the bidi manager: map the async
/// pipeline's [`ProcessedMessage`] into the unified [`Processed`] shape.
pub(crate) fn finish(processed: ProcessedMessage) -> Processed {
    Processed {
        message: processed.message,
        group_id: processed.group_id,
        next_cursor: processed.next_message,
        tried: processed.tried_to_process,
    }
}

/// Run a single raw group message through the shared processing pipeline.
///
/// 1. **Fast path:** if the message is already stored locally (e.g. a replayed
///    envelope after a cursor'd re-add — see XIP-83 client integration), return it
///    without decrypting.
/// 2. Otherwise run the full decrypt/store pipeline (which may trigger a recovery
///    sync for out-of-order / commit-dependent messages) and surface whatever it
///    produced.
///
/// The caller owns dedup and cursor advancement (see [`Processed`]). The live
/// [`super::stream_messages`] stream shares the same [`prepare`]/[`finish`] steps from
/// its poll state-machine (which cannot `.await` this directly).
#[xmtp_common::span(prefix = "stream")]
pub async fn process_one<'a>(
    factory: &impl ProcessFutureFactory<'a>,
    msg: xmtp_proto::types::GroupMessage,
) -> Result<Processed> {
    match prepare(factory, msg)? {
        Prepared::Ready {
            message,
            group_id,
            cursor,
        } => Ok(Processed {
            message: Some(message),
            group_id,
            next_cursor: cursor,
            tried: cursor,
        }),
        Prepared::NeedsProcessing(msg) => Ok(finish(factory.create(msg).await?)),
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
    use xmtp_proto::types::GroupId;

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
        use xmtp_common::Generate as _;

        let current_message = generate_message(current_message, &GroupId::generate());
        let mut mock_syncer = MockSync::new();
        let mut mock_db = MockGroupDatabase::new();
        let oid = current_message.originator_id();
        mock_db
            .expect_last_cursor()
            .times(1)
            .returning(move |_| Ok(Cursor::new(3, oid).into()));
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

        use crate::test::mock::generate_errored_summary;

        let current_message =
            generate_message(*success.first().unwrap_or(&55), &GroupId::generate());
        let mut mock_syncer = MockSync::new();
        let mut mock_db = MockGroupDatabase::new();
        let oid = current_message.originator_id();
        // the last cursor is
        mock_db
            .expect_last_cursor()
            .times(1)
            .returning(move |_| Ok(Cursor::new(50, oid).into()));
        // `lookup_stored_from_sync` may probe other successful cursors in the batch after the
        // primary lookup returns `None`.
        mock_db.expect_msg().times(1..).returning(|_, _| Ok(None));
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

    /// Regression test for missed stream messages after a valid error.
    ///
    /// Scenario: stream delivers envelope at cursor 10. Recovery sync produces
    /// `new_messages=[11]` and `errored={10, 12}`, so `last_errored() == 12`.
    /// Without the fallback in `lookup_stored_from_sync`, `process` returns
    /// `next_message = 12` and the subscriber never sees cursor 11 — the stream's
    /// per-group `has_seen` check drops envelope 11 when it later arrives.
    ///
    /// Commenting out the fallback loop in `lookup_stored_from_sync` will make
    /// this test fail: `processed.message` becomes `None` and `next_message` becomes 12.
    #[xmtp_common::test]
    pub async fn test_process_surfaces_decrypt_between_failed_cursors() {
        use xmtp_common::Generate as _;

        use crate::test::mock::{generate_errored_summary_with_group, generate_stored_msg};

        let gid = GroupId::generate();
        let stream_msg = generate_message(10, &gid);
        let oid = stream_msg.originator_id();
        let stored_11 = generate_stored_msg(Cursor::new(11, oid), gid);

        let mut mock_syncer = MockSync::new();
        let mut mock_db = MockGroupDatabase::new();

        // last_cursor < 10 so `needs_to_sync` triggers recovery.
        mock_db
            .expect_last_cursor()
            .times(1)
            .returning(move |_| Ok(Cursor::new(5, oid).into()));

        // Primary probe (id = None for cursor 10) misses; fallback probe for cursor 11 hits.
        let stored_for_mock = stored_11.clone();
        mock_db
            .expect_msg()
            .times(1..=2)
            .returning(move |id, _| match id {
                None => Ok(None),
                Some(i) if i.cursor.sequence_id == 11 => Ok(Some(stored_for_mock.clone())),
                Some(other) => panic!("unexpected probe for cursor {:?}", other.cursor),
            });

        mock_syncer.expect_process().times(1).returning(|_| {
            Err(SubscribeError::ReceiveGroup(Box::new(
                GroupMessageProcessingError::InvalidPayload,
            )))
        });
        mock_syncer
            .expect_recover()
            .times(1)
            .returning(move |_| generate_errored_summary_with_group(&gid, &[10, 12], &[11]));

        let processed = MessageProcessor::new(mock_syncer, mock_db)
            .process(stream_msg.clone())
            .await
            .unwrap();

        // Without the fallback: message would be None and next_message would be 12.
        assert!(processed.message.is_some());
        assert_eq!(processed.next_message, Cursor::new(11, oid));
        assert_eq!(processed.tried_to_process, stream_msg.cursor);
    }

    #[rstest]
    #[case(None)]
    #[case(Some(generate_stored_msg(Cursor::new(55, 0u32), xmtp_proto::types::GroupId::ZERO)))]
    #[xmtp_common::test]
    pub async fn test_cursor_no_sync(#[case] message: Option<StoredGroupMessage>) {
        let current_message = generate_message(55, &xmtp_proto::types::GroupId::ZERO);
        let mock_syncer = MockSync::new();
        let mut mock_db = MockGroupDatabase::new();
        let oid = current_message.originator_id();
        mock_db
            .expect_last_cursor()
            .times(1)
            .returning(move |_| Ok(Cursor::new(100, oid).into()));
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

    // A minimal in-test [`ProcessFutureFactory`] so we can drive `process_one` without
    // a real client: `retrieve` returns a canned DB hit/miss, `create` returns a canned
    // `ProcessedMessage` (and asserts it is only reached on a fast-path miss).
    struct StubFactory {
        retrieve: Option<StoredGroupMessage>,
        create_message: Option<StoredGroupMessage>,
        create_group: GroupId,
        create_next: Cursor,
        create_tried: Cursor,
        allow_create: bool,
    }

    impl<'a> ProcessFutureFactory<'a> for StubFactory {
        fn create(
            &self,
            _msg: xmtp_proto::types::GroupMessage,
        ) -> xmtp_common::BoxDynFuture<'a, Result<ProcessedMessage>> {
            assert!(
                self.allow_create,
                "process_one called create() on a DB fast-path hit"
            );
            let processed = ProcessedMessage {
                message: self.create_message.clone(),
                group_id: self.create_group,
                next_message: self.create_next,
                tried_to_process: self.create_tried,
            };
            Box::pin(async move { Ok(processed) })
        }

        fn retrieve(
            &self,
            _msg: &xmtp_proto::types::GroupMessage,
        ) -> Result<Option<StoredGroupMessage>> {
            Ok(self.retrieve.clone())
        }
    }

    /// A locally-stored message is returned without decrypting, and `create()` is
    /// never reached. The cursor advances to the message's own cursor.
    #[xmtp_common::test(unwrap_try = true)]
    async fn process_one_uses_db_fast_path_without_decrypting() {
        use xmtp_common::Generate as _;
        let gid = GroupId::generate();
        let msg = generate_message(55, &gid);
        let oid = msg.originator_id();
        let factory = StubFactory {
            retrieve: Some(generate_stored_msg(Cursor::new(55, oid), gid)),
            create_message: None,
            create_group: gid,
            create_next: Cursor::new(0, oid),
            create_tried: Cursor::new(0, oid),
            allow_create: false,
        };
        let processed = process_one(&factory, msg.clone()).await?;
        assert!(processed.message.is_some());
        assert_eq!(processed.next_cursor, msg.cursor);
        assert_eq!(processed.tried, msg.cursor);
        assert_eq!(processed.group_id, gid);
    }

    /// On a fast-path miss, `process_one` runs the full pipeline and surfaces the
    /// processed message and its post-processing cursor.
    #[xmtp_common::test(unwrap_try = true)]
    async fn process_one_runs_pipeline_on_fast_path_miss() {
        use xmtp_common::Generate as _;
        let gid = GroupId::generate();
        let msg = generate_message(10, &gid);
        let oid = msg.originator_id();
        let next = Cursor::new(11, oid);
        let factory = StubFactory {
            retrieve: None,
            create_message: Some(generate_stored_msg(next, gid)),
            create_group: gid,
            create_next: next,
            create_tried: msg.cursor,
            allow_create: true,
        };
        let processed = process_one(&factory, msg.clone()).await?;
        assert!(processed.message.is_some());
        assert_eq!(processed.next_cursor, next);
        assert_eq!(processed.tried, msg.cursor);
        assert_eq!(processed.group_id, gid);
    }
}
