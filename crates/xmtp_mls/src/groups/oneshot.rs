use super::{GroupError, MlsGroup, PreconfiguredPolicies};
use crate::context::XmtpSharedContext;
use xmtp_common::RetryableError;
use xmtp_common::snippet::Snippet;
use xmtp_db::{
    MlsProviderExt,
    TransactionOutcome::{self, Continue},
    XmtpMlsStorageProvider,
    consent_record::{ConsentState, conversation_consent},
    group::ConversationType,
    group_intent::IntentState,
    prelude::{QueryGroup, QueryGroupIntent, QueryReaddStatus},
};
use xmtp_id::AsIdRef;
use xmtp_mls_common::{group::GroupMetadataOptions, group_metadata::GroupMetadata};
use xmtp_proto::{
    types::{Cursor, GroupId},
    xmtp::mls::message_contents::{OneshotMessage, oneshot_message},
};

pub struct Oneshot {}

impl Oneshot {
    /// Creates a oneshot group with the given message and adds the specified inbox IDs to it.
    ///
    /// A oneshot group is a special type of group that contains a single message and is used
    /// for specific messaging scenarios like readd requests where no long-lived group is needed.
    ///
    /// # Arguments
    /// * `context` - The shared context for group operations
    /// * `inbox_ids` - List of inbox IDs to add to the group. Note that the sender's other
    ///   installations are implicitly included and will also receive the oneshot message.
    /// * `oneshot_message` - The oneshot message to include in the group metadata
    ///
    /// # Returns
    /// An error if sending failed, otherwise nothing
    pub async fn send_message<C: XmtpSharedContext, S: AsIdRef>(
        context: C,
        inbox_ids: impl AsRef<[S]>,
        oneshot_message: OneshotMessage,
    ) -> Result<(), GroupError> {
        Self::send_message_with_outcome(context, inbox_ids, oneshot_message)
            .await
            .map_err(|(error, _)| error)
    }

    pub(super) async fn send_message_with_outcome<C: XmtpSharedContext, S: AsIdRef>(
        context: C,
        inbox_ids: impl AsRef<[S]>,
        oneshot_message: OneshotMessage,
    ) -> Result<(), (GroupError, bool)> {
        // Create a oneshot group with the oneshot message
        let group = MlsGroup::<C>::create_and_insert(
            context.clone(),
            ConversationType::Oneshot,
            PreconfiguredPolicies::default().to_policy_set(),
            GroupMetadataOptions::default(),
            Some(oneshot_message),
        )
        .map_err(|error| (error, true))?;

        let sent = if !inbox_ids.as_ref().is_empty() {
            group.add_members(&inbox_ids).await.map(|_| ())
        } else {
            group.add_missing_installations().await.map(|_| ())
        };
        // A delayed follow-up can carry a rate limit after publication.
        let refused = sent.as_ref().err().is_some_and(|error| {
            !matches!(error, GroupError::PublishedButUnconfirmed { .. })
                && error.rate_limit_delay().is_some()
        });
        let cleanup = delete_oneshot_if_settled(&group, refused);
        match sent {
            Err(error) => Err((error, refused || cleanup.unwrap_or(false))),
            Ok(()) => cleanup.map(|_| ()).map_err(|error| (error, false)),
        }
    }

    pub fn process_message(
        provider: &impl MlsProviderExt,
        own_inbox_id: &str,
        sender_installation_id: Vec<u8>,
        message: OneshotMessage,
    ) -> Result<(), GroupError> {
        match message.message_type {
            Some(oneshot_message::MessageType::ReaddRequest(readd_request)) => {
                if Self::validate_readd_request(
                    provider.key_store(),
                    own_inbox_id,
                    &readd_request.group_id,
                    &sender_installation_id,
                )? {
                    let group_id = GroupId::try_from(readd_request.group_id.as_slice())?;
                    provider.key_store().db().update_requested_at_sequence_id(
                        &group_id,
                        &sender_installation_id,
                        readd_request.latest_commit_sequence_id as i64,
                    )?;
                    tracing::info!(
                        group_id = readd_request.group_id.snippet(),
                        sender_installation_id = sender_installation_id.snippet(),
                        latest_commit_sequence_id = readd_request.latest_commit_sequence_id,
                        "Stored incoming readd request"
                    );
                }
            }
            _ => {
                tracing::warn!(
                    "Oneshot message {:?} is not a recognized message type",
                    message.message_type
                );
            }
        }
        Ok(())
    }

    pub fn process_welcome(
        provider: &impl MlsProviderExt,
        own_inbox_id: &str,
        cursor: Cursor,
        sender_installation_id: Vec<u8>,
        metadata: GroupMetadata,
    ) -> Result<(), GroupError> {
        tracing::debug!("Processing oneshot welcome");
        if let Some(message) = metadata.oneshot_message {
            Self::process_message(provider, own_inbox_id, sender_installation_id, message)?;
        } else {
            tracing::warn!(
                "Oneshot group welcome {} does not have oneshot message",
                cursor
            );
        }
        Ok(())
    }

    // implements: FORK-053
    fn validate_readd_request(
        storage: &impl XmtpMlsStorageProvider,
        own_inbox_id: &str,
        group_id: &Vec<u8>,
        sender_installation_id: &Vec<u8>,
    ) -> Result<bool, GroupError> {
        let id = GroupId::try_from(group_id.as_slice())?;
        if conversation_consent(&storage.db(), own_inbox_id, &id)? != ConsentState::Allowed {
            tracing::warn!(
                group_id = group_id.snippet(),
                "Group not consented for readd request"
            );
            return Ok(false);
        }
        // Fetch the OpenMLS group without locking; consistency is not important here, and we are not writing to it
        let Ok(Some(openmls_group)) = openmls::group::MlsGroup::load(
            storage,
            &openmls::group::GroupId::from_slice(group_id.as_slice()),
        ) else {
            tracing::warn!(
                group_id = group_id.snippet(),
                "Unable to load OpenMLS group for readd request"
            );
            return Ok(false);
        };
        if !openmls_group
            .members()
            .any(|member| member.signature_key == *sender_installation_id)
        {
            tracing::warn!(
                group_id = group_id.snippet(),
                "Sender not a member of group for readd request"
            );
            return Ok(false);
        }

        Ok(true)
    }
}

// Returns whether a failed send permits a fresh request.
fn delete_oneshot_if_settled<C: XmtpSharedContext>(
    group: &MlsGroup<C>,
    refused: bool,
) -> Result<bool, GroupError> {
    if group.conversation_type != ConversationType::Oneshot {
        return Ok(false);
    }
    let intents = group
        .context
        .db()
        .find_group_intents(group.group_id, None, None)?;
    let unsent = intents.iter().all(|intent| {
        matches!(
            intent.state,
            IntentState::ToPublish | IntentState::Error | IntentState::Superseded
        )
    });
    if !refused
        && !unsent
        && intents.iter().any(|intent| {
            matches!(
                intent.state,
                IntentState::ToPublish | IntentState::Published | IntentState::Committed
            )
        })
    {
        return Ok(false);
    }
    crate::state_tx::state_write(group.context.mls_storage(), |tx| {
        let storage = tx.storage();
        let mut mls = openmls::group::MlsGroup::load(&storage, &group.group_id.to_openmls())?
            .ok_or(xmtp_db::StorageError::from(xmtp_db::NotFound::MlsGroup(
                group.group_id,
            )))?;
        mls.delete(&storage)?;
        storage.db().delete_oneshot_group(&group.group_id)?;
        Ok(Continue(unsent || refused))
    })
    .map(TransactionOutcome::into_continued)
}

#[cfg(all(test, not(target_arch = "wasm32")))]
#[allow(dead_code, clippy::duplicate_mod, clippy::unwrap_used)]
#[path = "mls_sync/publish/tests/faults.rs"]
mod send_faults;
#[cfg(all(test, not(target_arch = "wasm32")))]
use xmtp_proto::backend_v1::client_envelope::Payload;

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::send_faults::{
        Fault, FaultyApi, FaultyContext, faulty_group, group_message, welcome,
    };
    use super::*;
    use crate::groups::commit_log::CommitLogWorker;
    use crate::tester;
    use futures::stream::StreamExt;
    use xmtp_common::{
        RetryableError,
        time::{Duration, sleep},
    };
    use xmtp_db::{ConnectionExt, diesel::prelude::*, prelude::*};

    #[derive(QueryableByName, Debug, PartialEq)]
    struct Count {
        #[diesel(sql_type = diesel::sql_types::BigInt)]
        n: i64,
    }
    fn send_state(context: &impl XmtpSharedContext) -> Vec<Count> {
        context.db().raw_query(|conn| diesel::sql_query(
            "SELECT COUNT(*) AS n FROM groups UNION ALL SELECT COUNT(*) FROM group_intents UNION ALL SELECT COUNT(*) FROM group_messages UNION ALL SELECT COUNT(*) FROM local_commit_log UNION ALL SELECT COUNT(*) FROM remote_commit_log UNION ALL SELECT COUNT(*) FROM refresh_state UNION ALL SELECT COUNT(*) FROM incoming_envelopes UNION ALL SELECT COUNT(*) FROM openmls_key_value"
        ).load(conn)).expect("sender state counts")
    }
    const DELAY: Duration = Duration::from_millis(10);

    fn request_message(group_id: GroupId) -> OneshotMessage {
        OneshotMessage {
            message_type: Some(MessageType::ReaddRequest(ReaddRequest {
                group_id: group_id.to_vec(),
                latest_commit_sequence_id: 42,
            })),
        }
    }

    async fn readd_pair(published: bool, fail: bool) -> (MlsGroup<FaultyContext>, FaultyApi) {
        tester!(alix, disable_workers, enable_fork_recovery_requests);
        tester!(bo, disable_workers);
        let group = alix
            .find_or_create_dm(bo.inbox_id().to_string(), None)
            .await
            .expect("requester fixture");
        let mut worker = CommitLogWorker::new(alix.context.clone());
        worker._tick().await.expect("requester fixture");
        worker._tick().await.expect("requester fixture");
        let faults = if fail {
            vec![Fault::RateLimit(DELAY); 18]
        } else {
            vec![]
        };
        let target = if published { welcome } else { group_message };
        let api = FaultyApi::new(&alix, target, faults);
        let faulty = faulty_group(&alix, &group.group_id, api.clone()).await;
        let db = faulty.context.db();
        db.set_group_commit_log_forked_status(&group.group_id, Some(true))
            .expect("requester fixture");
        (faulty, api)
    }
    use xmtp_proto::xmtp::mls::message_contents::{ReaddRequest, oneshot_message::MessageType};

    // verifies: FORK-051, SEND-007, SEND-015
    #[rstest::rstest]
    #[case::success(false, false, false)]
    #[case::intent_insert(false, false, true)]
    #[case::refused(false, true, false)]
    #[case::published_but_unconfirmed(true, true, false)]
    #[xmtp_common::test(unwrap_try = true)]
    async fn oneshot_readd_send_outcomes(
        #[case] published: bool,
        #[case] fail: bool,
        #[case] trigger: bool,
    ) -> Result<(), xmtp_common::BoxDynError> {
        let (group, api) = readd_pair(published, fail).await;
        let context = &group.context;
        let db = context.db();
        let before = send_state(context);
        if trigger {
            db.raw_query(|conn| diesel::sql_query("CREATE TRIGGER fail_intent BEFORE INSERT ON group_intents BEGIN SELECT RAISE(ABORT, 'request not recorded before sending') WHERE NOT EXISTS (SELECT 1 FROM readd_status WHERE requested_at_sequence_id IS NOT NULL); SELECT RAISE(ABORT, 'injected intent failure'); END").execute(conn))?;
        }
        let mut worker = CommitLogWorker::new(group.context.clone());
        let result = worker._tick().await;
        assert_eq!(result.is_err(), fail || trigger);
        if trigger {
            assert!(format!("{result:?}").contains("injected intent failure"));
        }
        if fail {
            assert_eq!(result.unwrap_err().rate_limit_delay(), Some(DELAY));
        }
        let awaiting = || {
            db.is_awaiting_readd(&group.group_id, context.installation_id().as_slice())
                .unwrap()
        };
        let keep = published || !fail && !trigger;
        assert_eq!(awaiting(), keep, "requester latch");
        let sent = api.requests.lock().len();
        if published {
            use xmtp_db::schema::groups;
            let id = db.raw_query(|conn| {
                groups::table
                    .filter(groups::conversation_type.eq(ConversationType::Oneshot))
                    .select(groups::id)
                    .first::<GroupId>(conn)
            })?;
            let intent = db.find_group_intents(id, None, None)?.pop().unwrap();
            assert_eq!(intent.state, IntentState::Committed);
            let bytes = db.prepared_envelopes(intent.id)?.unwrap();
            worker._tick().await?;
            assert_eq!(api.requests.lock().len(), sent);
            assert_eq!(db.prepared_envelopes(intent.id)?.unwrap(), bytes);
            let temporary = MlsGroup::new_cached(context.clone(), &id)?.0;
            temporary.sync().await?;
            // A settled send can still have later queued input.
            db.raw_query(|conn| diesel::sql_query("INSERT INTO incoming_envelopes (entity_id, entity_kind, sequence_id, envelope, blocked, retry_at_ns, envelope_bytes) VALUES (?, 2, 999999999, X'01', 1, 9223372036854775807, 1)").bind::<diesel::sql_types::Binary, _>(id.as_slice()).execute(conn))?;
            assert!(delete_oneshot_if_settled(&temporary, false).is_ok());
            assert_eq!(send_state(context), before);
        } else {
            assert_eq!(send_state(context), before);
            if trigger {
                db.raw_query(|conn| diesel::sql_query("DROP TRIGGER fail_intent").execute(conn))?;
            }
            sleep(DELAY).await;
            worker._tick().await?;
            assert_eq!(api.requests.lock().len() > sent, fail || trigger);
        }
        Ok(())
    }

    // verifies: FORK-052
    #[xmtp_common::test(unwrap_try = true)]
    async fn test_receive_oneshot_message_via_syncing() {
        tester!(alix, disable_workers);
        tester!(bo, disable_workers);
        tester!(caro, disable_workers);
        tester!(phone, from: alix, disable_workers);
        let a_group = alix
            .create_group_with_members(&[bo.inbox_id(), caro.inbox_id()], None, None)
            .await?;
        let group_id = a_group.group_id;
        let own = alix.context.installation_id();
        for recipient in [&bo, &caro] {
            recipient.sync_all_welcomes_and_groups(None).await?;
            recipient
                .group(&group_id)?
                .update_consent_state(ConsentState::Allowed)?;
            let db = recipient.context.db();
            assert!(db.get_readd_status(&group_id, own.as_slice())?.is_none());
        }
        phone.sync_all_welcomes_and_groups(None).await?;
        phone
            .group(&group_id)?
            .update_consent_state(ConsentState::Allowed)?;
        let before = send_state(&alix.context);
        let message = request_message(group_id);
        Oneshot::send_message(alix.context.clone(), Vec::<String>::new(), message).await?;
        assert_eq!(send_state(&alix.context), before);
        phone.sync_welcomes().await?;
        let status = phone
            .context
            .db()
            .get_readd_status(&group_id, own.as_slice())?
            .unwrap();
        assert_eq!(status.requested_at_sequence_id, Some(42));
        assert_eq!(status.responded_at_sequence_id, None);
        Oneshot::send_message(
            alix.context.clone(),
            [bo.inbox_id(), caro.inbox_id()],
            request_message(group_id),
        )
        .await?;
        assert_eq!(send_state(&alix.context), before);
        for recipient in [&bo, &caro] {
            recipient.sync_welcomes().await?;
            assert_eq!(recipient.find_groups(Default::default())?.len(), 1);
            let db = recipient.context.db();
            let status = db.get_readd_status(&group_id, own.as_slice())?.unwrap();
            assert_eq!(status.requested_at_sequence_id, Some(42));
            assert_eq!(status.responded_at_sequence_id, None);
        }
    }

    // verifies: FORK-052
    #[xmtp_common::test(unwrap_try = true)]
    async fn test_oneshot_groups_not_in_find_groups() {
        tester!(alix);

        // Keep a stored one-shot: sender cleanup must not bypass the listing filter.
        MlsGroup::create_and_insert(
            alix.context.clone(),
            ConversationType::Oneshot,
            PreconfiguredPolicies::default().to_policy_set(),
            GroupMetadataOptions::default(),
            None,
        )?;
        let normal = alix.create_group(None, None)?;
        let alix_groups = alix.find_groups(Default::default()).unwrap();

        // Oneshot groups should not appear in the regular groups list
        assert_eq!(alix_groups.len(), 1);
        assert_eq!(alix_groups[0].group_id, normal.group_id);
    }

    #[tokio::test]
    async fn test_oneshot_groups_not_in_stream_groups() {
        tester!(alix);
        tester!(bo);

        // Subscribe to conversation events
        let mut alix_conversations = alix
            .stream_conversations(None, false)
            .await
            .expect("Failed to stream conversations");
        let mut bo_conversations = bo
            .stream_conversations(None, false)
            .await
            .expect("Failed to stream conversations");

        // Create a test oneshot message
        let readd_request = ReaddRequest {
            group_id: vec![5, 6, 7, 8],
            latest_commit_sequence_id: 0,
        };
        let oneshot_message = OneshotMessage {
            message_type: Some(MessageType::ReaddRequest(readd_request)),
        };

        // Alix sends the oneshot message to Bo
        Oneshot::send_message(alix.context.clone(), vec![bo.inbox_id()], oneshot_message)
            .await
            .expect("Failed to send oneshot message");

        // Small delay to ensure any events would have been processed
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Try to get any conversation events with a timeout
        let alix_event = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            alix_conversations.next(),
        )
        .await;
        let bo_event = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            bo_conversations.next(),
        )
        .await;

        // Both should timeout (no new conversations)
        assert!(
            alix_event.is_err(),
            "Alix should not receive new conversation event"
        );
        assert!(
            bo_event.is_err(),
            "Bo should not receive new conversation event"
        );
    }

    #[tokio::test]
    async fn test_syncing_and_streaming_oneshot_group_simultaneously() {
        // Test syncing and streaming simultaneously, which causes Welcome to be processed twice
        // Note that on the second time, there is no group in the DB to refetch by ID - this should
        // not surface an error in the stream
        tester!(alix);
        tester!(bo);

        let mut bo_conversations = bo
            .stream_conversations(None, false)
            .await
            .expect("Failed to stream conversations");

        // Create a test oneshot message
        let readd_request = ReaddRequest {
            group_id: vec![5, 6, 7, 8],
            latest_commit_sequence_id: 0,
        };
        let oneshot_message = OneshotMessage {
            message_type: Some(MessageType::ReaddRequest(readd_request)),
        };

        // Alix sends the oneshot message to Bo
        Oneshot::send_message(alix.context.clone(), vec![bo.inbox_id()], oneshot_message)
            .await
            .expect("Failed to send oneshot message");

        // Bo syncs welcomes to receive any oneshot groups
        bo.sync_welcomes().await.expect("Failed to sync welcomes");

        // Small delay to ensure any events would have been processed
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let bo_event = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            bo_conversations.next(),
        )
        .await;

        assert!(
            bo_event.is_err(),
            "Bo should not receive new conversation event"
        );
    }
}
