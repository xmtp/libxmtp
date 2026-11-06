//! Concurrent sync, cursors, and fork detection.

use super::*;
use crate::groups::test_support::{EnginePeer, EngineSuiteOps, EngineTestPeer, EngineTestPeerOps};

// verifies: PROC-061
#[xmtp_common::test(unwrap_try = true)]
async fn far_forward_application_message_is_rejected_and_later_message_arrives() {
    use crate::groups::mls_sync::{GroupHeadOutcome, GroupMessageProcessingError};
    use xmtp_db::{
        TransactionOutcome::Continue,
        incoming_envelope::{QueryIncomingEnvelope, StreamTopic},
    };
    use xmtp_mls_core::state_tx::state_write;
    use xmtp_proto::types::{OrderedEnvelopeBatch, Topic};
    use xmtp_proto::xmtp::mls::message_contents::{
        ContentTypeId, EncodedContent,
        plaintext_envelope::{Content, V1},
    };

    const LAST_ACCEPTABLE_GENERATION: u32 = 1_000;

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let caro = EngineTestPeer::new().await;
    let alix_group = alix.create_group(None, None)?;
    alix_group
        .add_members(&[bo.inbox_id(), caro.inbox_id()])
        .await?;
    let bo_group = receive_group_invite(&bo).await;
    let caro_group = receive_group_invite(&caro).await;
    bo_group.receive().await?;
    caro_group.receive().await?;

    let topic = StreamTopic::group(bo_group.group_id);
    let before = bo.context.db().topic_progress(&topic)?;
    let before_count = bo_group.find_messages(&MsgQueryArgs::default())?.len();

    // Prepare an honest peer's valid ciphertext before the attack, then
    // publish it later. A normal send would first sync the attacker's head.
    let honest_content = EncodedContent {
        r#type: Some(ContentTypeId {
            authority_id: "xmtp.org".into(),
            type_id: "text".into(),
            version_major: 1,
            version_minor: 0,
        }),
        content: b"valid message behind the head".to_vec(),
        ..Default::default()
    }
    .encode_to_vec();
    let honest_payload = PlaintextEnvelope {
        content: Some(Content::V1(V1 {
            content: honest_content.clone(),
            idempotency_key: "honest-peer".into(),
        })),
    }
    .encode_to_vec();
    let honest_message = state_write(caro.context.mls_storage(), |tx| {
        tx.with_group(caro_group.group_id, |mls_group, storage| {
            let mut last = None;
            for _ in 0..=LAST_ACCEPTABLE_GENERATION {
                let message = mls_group.create_message(
                    &XmtpOpenMlsProviderRef::new(storage),
                    &caro.context.identity().installation_keys,
                    &honest_payload,
                )?;
                last = Some(message.to_bytes()?);
            }
            Ok::<_, GroupError>(Continue(last.expect("at least one ciphertext")))
        })
    })?
    .into_continued();
    let honest_units = caro_group.prepare_group_messages(vec![(&honest_message, false)])?;

    // Generation 1001 is beyond the window; Carol's generation 1000 is at its edge.
    let payload = PlaintextEnvelope { content: None }.encode_to_vec();
    let distant_message = state_write(alix.context.mls_storage(), |tx| {
        tx.with_group(alix_group.group_id, |mls_group, storage| {
            let mut last = None;
            for _ in 0..=LAST_ACCEPTABLE_GENERATION + 1 {
                let message = mls_group.create_message(
                    &XmtpOpenMlsProviderRef::new(storage),
                    &alix.context.identity().installation_keys,
                    &payload,
                )?;
                last = Some(message.to_bytes()?);
            }
            Ok::<_, GroupError>(Continue(last.expect("at least one ciphertext")))
        })
    })?
    .into_continued();
    alix.context
        .api()
        .send_group_messages(alix_group.prepare_group_messages(vec![(&distant_message, false)])?)
        .await?;
    caro.context.api().send_group_messages(honest_units).await?;

    let wire_topic = Topic::new_group_message(bo_group.group_id);
    let envelopes = bo
        .context
        .api()
        .query_all(
            [(wire_topic.clone(), before.received)].into(),
            bo.context.api().limits().max_query_limit as u32,
        )
        .await?;
    assert_eq!(envelopes.len(), 2);
    bo.mls_store().admit_incoming_batch(
        &OrderedEnvelopeBatch {
            topic: wire_topic,
            after: before.received,
            envelopes: envelopes.into_iter().map(Into::into).collect(),
        },
        bo.context
            .incoming_runtime()
            .policy()
            .incoming_limits(topic.kind),
    )?;
    let db = bo.context.db();
    let received = db.topic_progress(&topic)?.received;
    let first_cursor = Cursor(db.first_pending_envelope(&topic)?.unwrap().sequence_id as u64);
    let GroupHeadOutcome::Progress {
        cursor,
        result: Err(GroupMessageProcessingError::FarForwardSender),
    } = bo_group.process_pending_group_head(None)?
    else {
        panic!("far-forward application ciphertext must be terminally rejected");
    };
    assert_eq!(cursor, first_cursor);
    let rejection = db.read_last_rejection(&topic)?.unwrap();
    assert_eq!(rejection.sequence_id, cursor);
    assert_eq!(rejection.code, "far_forward_generation");
    assert_eq!(db.topic_progress(&topic)?.processed, cursor);
    assert_eq!(db.pending_states_through(&topic, received)?.len(), 1);
    assert_eq!(
        bo_group.find_messages(&MsgQueryArgs::default())?.len(),
        before_count
    );

    assert!(matches!(
        bo_group.process_pending_group_head(None)?,
        GroupHeadOutcome::Progress { result: Ok(_), cursor } if cursor == received
    ));
    assert_eq!(db.topic_progress(&topic)?.processed, received);
    assert!(db.first_pending_envelope(&topic)?.is_none());
    let messages = bo_group.find_messages(&MsgQueryArgs::default())?;
    assert_eq!(messages.len(), before_count + 1);
    assert_eq!(
        messages.last().unwrap().decrypted_message_bytes,
        honest_content
    );
    assert_eq!(db.read_last_rejection(&topic)?, Some(rejection));
}

/// A proposal and then a commit whose sender generations are beyond the
/// forward window are terminal rejections, so they cannot hold the group
/// head. The rejected commit is logged as undecryptable at the current
/// epoch, and the group is not marked as forked.
// verifies: PROC-061
#[xmtp_common::test(unwrap_try = true)]
async fn far_forward_proposal_is_rejected() {
    use crate::groups::{
        mls_ext::commit_without_group_info,
        mls_sync::{GroupHeadOutcome, GroupMessageProcessingError, generate_prepared_commit},
    };
    use openmls::prelude::{LeafNodeParameters, tls_codec::Serialize as _};
    use openmls_traits::OpenMlsProvider as _;
    use xmtp_db::{
        TransactionOutcome::Continue,
        incoming_envelope::{QueryIncomingEnvelope, StreamTopic},
        local_commit_log::QueryLocalCommitLog,
        remote_commit_log::CommitResult,
    };
    use xmtp_mls_core::state_tx::state_write;
    use xmtp_proto::types::{OrderedEnvelopeBatch, Topic};

    const PROPOSAL_COUNT: u32 = 1_002;

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;
    let bo_group = receive_group_invite(&bo).await;
    // A rejected commit right after the Welcome is not logged.
    alix_group.update_group_name("logged".into()).await?;
    bo_group.receive().await?;
    let topic = StreamTopic::group(bo_group.group_id);
    let before = bo.context.db().topic_progress(&topic)?;
    let signer = alix.context.identity().installation_keys.clone();
    let (proposal, commit) = state_write(alix.context.mls_storage(), |tx| {
        tx.with_group(alix_group.group_id, |mls_group, storage| {
            let mut last = None;
            for _ in 0..PROPOSAL_COUNT {
                let (message, _) = mls_group
                    .propose_self_update(
                        &XmtpOpenMlsProviderRef::new(storage),
                        &signer,
                        LeafNodeParameters::default(),
                    )
                    .unwrap();
                last = Some(message.to_bytes()?);
            }
            let (commit, _, _) = generate_prepared_commit(storage, mls_group, |mls, provider| {
                let bundle = commit_without_group_info(mls, provider.storage(), |builder| {
                    builder.consume_proposal_store(false)
                })
                .unwrap()
                .build(provider.rand(), provider.crypto(), &signer, |_| true)
                .unwrap()
                .stage_commit(provider)
                .unwrap();
                bundle.into_messages().0.tls_serialize_detached()
            })?;
            Ok::<_, GroupError>(Continue((last.expect("at least one proposal"), commit)))
        })
    })?
    .into_continued();
    alix.context
        .api()
        .send_group_messages(
            alix_group.prepare_group_messages(vec![(&proposal, false), (&commit, false)])?,
        )
        .await?;

    let wire_topic = Topic::new_group_message(bo_group.group_id);
    let envelopes = bo
        .context
        .api()
        .query_all(
            [(wire_topic.clone(), before.received)].into(),
            bo.context.api().limits().max_query_limit as u32,
        )
        .await?;
    assert_eq!(envelopes.len(), 2);
    bo.mls_store().admit_incoming_batch(
        &OrderedEnvelopeBatch {
            topic: wire_topic,
            after: before.received,
            envelopes: envelopes.into_iter().map(Into::into).collect(),
        },
        bo.context
            .incoming_runtime()
            .policy()
            .incoming_limits(topic.kind),
    )?;
    let db = bo.context.db();
    let received = db.topic_progress(&topic)?.received;
    let authenticator = bo_group.epoch_authenticator().await?;
    for _ in 0..2 {
        let GroupHeadOutcome::Progress {
            cursor,
            result: Err(GroupMessageProcessingError::FarForwardSender),
        } = bo_group.process_pending_group_head(None)?
        else {
            panic!("a far-forward handshake must be terminally rejected");
        };
        let rejection = db.read_last_rejection(&topic)?.unwrap();
        assert_eq!(rejection.sequence_id, cursor);
        assert_eq!(rejection.code, "far_forward_generation");
        assert_eq!(db.topic_progress(&topic)?.processed, cursor);
    }
    assert_eq!(db.topic_progress(&topic)?.processed, received);
    assert!(db.first_pending_envelope(&topic)?.is_none());
    let log = db.get_latest_log_for_group(&bo_group.group_id)?.unwrap();
    assert_eq!(log.commit_result, CommitResult::Undecryptable);
    assert_eq!(
        log.last_epoch_authenticator,
        log.applied_epoch_authenticator
    );
    assert_eq!(log.applied_epoch_authenticator, authenticator);
    assert_eq!(bo_group.epoch_authenticator().await?, authenticator);
    assert!(!bo_group.debug_info().await?.maybe_forked);
}

// verifies: EVENT-001, EVENT-010
#[xmtp_common::test(unwrap_try = true)]
async fn process_messages_abort_on_retryable_error() {
    use crate::groups::mls_sync::GroupHeadOutcome;
    use xmtp_db::incoming_envelope::{IncomingRetry, QueryIncomingEnvelope, StreamTopic};
    use xmtp_proto::types::{OrderedEnvelopeBatch, Topic};

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;
    let bo_group = receive_group_invite(&bo).await;
    bo_group.receive().await?;
    let topic = StreamTopic::group(bo_group.group_id);
    let before = bo.context.db().topic_progress(&topic)?;
    let message_count = bo_group.find_messages(&MsgQueryArgs::default())?.len();
    for text in [b"first".as_slice(), b"second".as_slice()] {
        alix_group
            .send_message(text, SendMessageOpts::default())
            .await?;
    }
    let wire_topic = Topic::new_group_message(bo_group.group_id);
    let envelopes = bo
        .context
        .api()
        .query_all(
            [(wire_topic.clone(), before.received)].into(),
            bo.context.api().limits().max_query_limit as u32,
        )
        .await?;
    assert_eq!(envelopes.len(), 2);
    bo.mls_store().admit_incoming_batch(
        &OrderedEnvelopeBatch {
            topic: wire_topic,
            after: before.received,
            envelopes: envelopes.into_iter().map(Into::into).collect(),
        },
        bo.context
            .incoming_runtime()
            .policy()
            .incoming_limits(topic.kind),
    )?;
    let db = bo.context.store().db();
    let received = db.topic_progress(&topic)?.received;
    let message_events = bo.context.events().subscribe(
        xmtp_events::EventFilter::new([xmtp_events::EventKind::MessageReceived]),
        Some(10),
    );
    let crypto_before = bo.context.mls_storage().hash_all()?;
    db.raw_query(|conn| {
        conn.batch_execute(
            "CREATE TRIGGER abort_pending_completion BEFORE DELETE ON incoming_envelopes \
         BEGIN SELECT RAISE(FAIL, 'test completion failure'); END;",
        )
    })?;
    assert!(matches!(
        bo_group.process_pending_group_head(None)?,
        GroupHeadOutcome::Waiting { blocked: false, .. }
    ));
    assert_eq!(db.topic_progress(&topic)?.processed, before.processed);
    assert_eq!(db.pending_states_through(&topic, received)?.len(), 2);
    assert_eq!(
        bo_group.find_messages(&MsgQueryArgs::default())?.len(),
        message_count
    );
    assert_eq!(bo.context.mls_storage().hash_all()?, crypto_before);
    assert!(
        message_events.drain().is_empty(),
        "rolled-back emit escaped"
    );

    db.raw_query(|conn| conn.batch_execute("DROP TRIGGER abort_pending_completion"))?;
    let head = db.first_pending_envelope(&topic)?.unwrap();
    db.set_incoming_retry(
        &topic,
        Cursor(head.sequence_id as u64),
        &IncomingRetry {
            retry_at_ns: 0,
            blocked: false,
            error_code: None,
            retry_expires_at_ns: None,
        },
    )?;
    for _ in 0..2 {
        assert!(matches!(
            bo_group.process_pending_group_head(None)?,
            GroupHeadOutcome::Progress { result: Ok(_), .. }
        ));
    }
    assert_eq!(db.topic_progress(&topic)?.processed, received);
    assert!(db.first_pending_envelope(&topic)?.is_none());
    assert_eq!(
        bo_group.find_messages(&MsgQueryArgs::default())?.len(),
        message_count + 2
    );
    let published = message_events.drain();
    assert_eq!(published.len(), 2);
    assert!(published.iter().all(|event| matches!(
        &event.client,
        Some(xmtp_events::ClientEvent::MessageReceived(_))
    )));
    assert!(matches!(
        bo_group.process_pending_group_head(None)?,
        GroupHeadOutcome::Idle
    ));
    assert!(message_events.drain().is_empty());
}

#[xmtp_common::test(unwrap_try = true)]
async fn skip_already_processed_messages() {
    use crate::groups::mls_sync::GroupHeadOutcome;
    use xmtp_db::incoming_envelope::{QueryIncomingEnvelope, StreamTopic};
    use xmtp_proto::types::{OrderedEnvelopeBatch, Topic};

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;
    let bo_group = receive_group_invite(&bo).await;
    bo_group.receive().await?;
    let topic = StreamTopic::group(bo_group.group_id);
    let wire_topic = Topic::new_group_message(bo_group.group_id);
    let limits = bo
        .context
        .incoming_runtime()
        .policy()
        .incoming_limits(topic.kind);
    let initial_count = bo_group.find_messages(&MsgQueryArgs::default())?.len();
    for expected_count in 1..=2 {
        alix_group
            .send_message(&[1], SendMessageOpts::default())
            .await?;
        let envelopes = bo
            .context
            .api()
            .query_all(
                [(wire_topic.clone(), Cursor(0))].into(),
                bo.context.api().limits().max_query_limit as u32,
            )
            .await?;
        let batch = OrderedEnvelopeBatch {
            topic: wire_topic.clone(),
            after: Cursor(0),
            envelopes: envelopes.into_iter().map(Into::into).collect(),
        };
        let admission = bo.mls_store().admit_incoming_batch(&batch, limits)?;
        assert_eq!(admission.inserted, 1);
        assert!(matches!(
            bo_group.process_pending_group_head(None)?,
            GroupHeadOutcome::Progress { result: Ok(_), .. }
        ));
        assert_eq!(
            bo.mls_store()
                .admit_incoming_batch(&batch, limits)?
                .inserted,
            0
        );
        let progress = bo.context.db().topic_progress(&topic)?;
        assert_eq!(progress.received, admission.received);
        assert_eq!(progress.processed, admission.received);
        assert!(bo.context.db().first_pending_envelope(&topic)?.is_none());
        assert_eq!(
            bo_group.find_messages(&MsgQueryArgs::default())?.len(),
            initial_count + expected_count
        );
    }
}

#[xmtp_common::test]
async fn skip_already_processed_intents() {
    let alix = EngineTestPeer::new_with(|o| o.triggers = true).await;

    let bo_client = EngineTestPeer::new_with(|o| o.triggers = true).await;

    let alix_group = alix.create_group(None, None).unwrap();

    alix_group
        .add_members(&[bo_client.inbox_id()])
        .await
        .unwrap();

    bo_client.sync_welcomes().await.unwrap();
    let bo_groups = bo_client.find_groups(GroupQueryArgs::default()).unwrap();
    let bo_group = bo_groups.first().unwrap();
    bo_group
        .send_message(&[2], SendMessageOpts::default())
        .await
        .unwrap();
    let intent = bo_client
        .context
        .db()
        .find_group_intents(bo_group.group_id, Some(vec![IntentState::Processed]), None)
        .unwrap();
    assert_eq!(intent.len(), 2); //key_update and send_message

    let process_result = bo_group.sync_until_intent_resolved(intent[1].id).await;
    assert_ok!(process_result);
}

#[rstest::rstest]
#[xmtp_common::test(flavor = "multi_thread")]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn add_missing_installs_reentrancy() {
    let alix1 = EngineTestPeer::new().await;
    let alix1_group = alix1.create_group(None, None).unwrap();

    let alix2 = alix1.new_installation().await;

    // We are going to run add_missing_installations TWICE
    // which will create two intents to add the installations
    create_membership_update_no_sync(&alix1_group).await;
    create_membership_update_no_sync(&alix1_group).await;

    // Now I am going to run publish intents multiple times
    alix1_group
        .publish_intents()
        .await
        .expect("Expect publish to be OK");
    alix1_group
        .publish_intents()
        .await
        .expect("Expected publish to be OK");

    // Now I am going to sync twice
    alix1_group.sync_with_conn().await.unwrap();
    let settled_authenticator = alix1_group.epoch_authenticator().await.unwrap();
    alix1_group.sync_with_conn().await.unwrap();
    assert_eq!(
        alix1_group.epoch_authenticator().await.unwrap(),
        settled_authenticator
    );

    // Make sure that only one welcome was sent
    let alix2_welcomes = alix1
        .context
        .api()
        .query_welcome_messages(alix2.installation_public_key())
        .await
        .unwrap();
    assert_eq!(alix2_welcomes.len(), 1);

    let alix2_group = receive_group_invite(&alix2).await;
    assert_eq!(
        alix2_group.epoch_authenticator().await.unwrap(),
        settled_authenticator
    );
    for group in [&alix1_group, &alix2_group] {
        let installations = group
            .load_mls_group_with_lock(group.context.mls_storage(), |mls_group| {
                Ok(mls_group.members().count())
            })
            .unwrap();
        assert_eq!(installations, 2);
    }
    assert!(
        alix1
            .db()
            .find_group_intents(
                alix1_group.group_id,
                Some(vec![
                    IntentState::ToPublish,
                    IntentState::Published,
                    IntentState::Error
                ]),
                None,
            )
            .unwrap()
            .is_empty()
    );

    // Send a message from alix1
    alix1_group
        .send_message("hi from alix1".as_bytes(), SendMessageOpts::default())
        .await
        .unwrap();
    // Send a message from alix2
    alix2_group
        .send_message("hi from alix2".as_bytes(), SendMessageOpts::default())
        .await
        .unwrap();

    // Sync both clients
    alix1_group.sync().await.unwrap();
    alix2_group.sync().await.unwrap();

    let alix1_messages = alix1_group.find_messages(&MsgQueryArgs::default()).unwrap();
    let alix2_messages = alix2_group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(alix1_messages.len(), alix2_messages.len() - 1);

    assert!(
        alix1_messages
            .iter()
            .any(|m| m.decrypted_message_bytes == "hi from alix2".as_bytes())
    );
    assert!(
        alix2_messages
            .iter()
            .any(|m| m.decrypted_message_bytes == "hi from alix1".as_bytes())
    );
}

#[cfg(not(target_arch = "wasm32"))]
// verifies: FORK-073
#[xmtp_common::test(unwrap_try = true, flavor = "multi_thread")]
async fn test_when_processing_message_return_future_wrong_epoch_group_marked_probably_forked() {
    use crate::groups::test_mocks::set_test_mode_future_wrong_epoch;

    let client_a = EngineTestPeer::new().await;
    let client_b = EngineTestPeer::new().await;

    let group_a = client_a.create_group(None, None).unwrap();
    group_a.add_members(&[client_b.inbox_id()]).await.unwrap();

    client_b.sync_welcomes().await.unwrap();

    let binding = client_b.find_groups(GroupQueryArgs::default()).unwrap();
    let group_b = binding.first().unwrap();

    group_a
        .send_message(&[1], SendMessageOpts::default())
        .await
        .unwrap();
    set_test_mode_future_wrong_epoch(true);
    group_b.sync().await.unwrap();
    set_test_mode_future_wrong_epoch(false);
    let group_debug_info = group_b.debug_info().await.unwrap();
    assert!(group_debug_info.maybe_forked);
    assert!(!group_debug_info.fork_details.is_empty());
    let topic = xmtp_db::incoming_envelope::StreamTopic::group(group_b.group_id);
    let db = client_b.context.db();
    let rejection = db.read_last_rejection(&topic)??;
    assert_eq!(rejection.code, "mls_processing_failure");
    assert_eq!(db.topic_progress(&topic)?.processed, rejection.sequence_id);
    assert!(db.first_pending_envelope(&topic)?.is_none());

    group_a
        .send_message(&[2], SendMessageOpts::default())
        .await?;
    group_b.sync().await?;
    assert_eq!(group_b.test_last_message_bytes().await??, vec![2]);
    assert!(db.topic_progress(&topic)?.processed > rejection.sequence_id);
    client_b
        .context
        .db()
        .clear_fork_flag_for_group(&group_b.group_id)
        .unwrap();
    let group_debug_info = group_b.debug_info().await.unwrap();
    assert!(!group_debug_info.maybe_forked);
    assert!(group_debug_info.fork_details.is_empty());
}

#[xmtp_common::test(unwrap_try = true)]
async fn own_message_without_intent_skips_and_increments_cursor() {
    use xmtp_db::TransactionOutcome::Continue;
    use xmtp_db::incoming_envelope::{QueryIncomingEnvelope, StreamTopic};
    use xmtp_mls_core::state_tx::state_write;

    let alice = EngineTestPeer::new().await;
    let group = alice.create_group(None, None)?;
    group.key_update().await?;
    let topic = StreamTopic::group(group.group_id);
    let before = alice.context.db().topic_progress(&topic)?;
    let initial_count = group.find_messages(&MsgQueryArgs::default())?.len();

    // Create own ciphertext without a matching intent. Own ratchets cannot decrypt it.
    let payload = PlaintextEnvelope { content: None }.encode_to_vec();
    let message = state_write(alice.context.mls_storage(), |tx| {
        tx.with_group(group.group_id, |mls_group, storage| {
            let message = mls_group.create_message(
                &XmtpOpenMlsProviderRef::new(storage),
                &alice.context.identity().installation_keys,
                &payload,
            )?;
            Ok::<_, GroupError>(Continue(message.to_bytes()?))
        })
    })?
    .into_continued();
    alice
        .context
        .api()
        .send_group_messages(group.prepare_group_messages(vec![(&message, false)])?)
        .await?;
    group.receive().await?;

    let db = alice.context.db();
    let rejected = db.read_last_rejection(&topic)?.unwrap();
    assert_eq!(rejected.code, "own_message_without_attempt");
    assert!(rejected.sequence_id > before.processed);
    let progress = db.topic_progress(&topic)?;
    assert_eq!(progress.processed, rejected.sequence_id);
    assert_eq!(progress.received, rejected.sequence_id);
    assert!(db.first_pending_envelope(&topic)?.is_none());
    assert_eq!(
        group.find_messages(&MsgQueryArgs::default())?.len(),
        initial_count
    );

    group
        .send_message(b"after terminal rejection", SendMessageOpts::default())
        .await?;
    assert!(db.topic_progress(&topic)?.processed > rejected.sequence_id);
    assert_eq!(db.read_last_rejection(&topic)?, Some(rejected));
    assert_eq!(
        group.find_messages(&MsgQueryArgs::default())?.len(),
        initial_count + 1
    );
}

#[xmtp_common::test(unwrap_try = true)]
// A test drives an OpenMLS convenience commit; its GroupInfo never leaves the test.
#[allow(clippy::disallowed_methods)]
async fn prepared_commit_keeps_keys_without_advancing_epoch() {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let group = alix.create_group(None, None)?;
    group.add_members(&[bo.inbox_id()]).await?;
    group.sync().await?;

    let storage = alix.context.mls_storage();
    let start_hash = storage.hash_all()?;
    let start_epoch = group.epoch().await?;
    let installation_keys = &group.context.identity().installation_keys;
    xmtp_mls_core::state_tx::state_write(storage, |tx| {
        tx.with_group(group.group_id, |mls_group, storage| {
            let (_, staged_commit, epoch) = crate::groups::mls_sync::generate_prepared_commit(
                storage,
                mls_group,
                |group, provider| {
                    group.self_update(
                        provider,
                        installation_keys,
                        openmls::treesync::LeafNodeParameters::default(),
                    )
                },
            )?;
            assert!(staged_commit.is_some());
            assert_eq!(epoch, start_epoch);
            Ok::<_, GroupError>(xmtp_db::TransactionOutcome::Continue(()))
        })
    })?;

    assert_ne!(start_hash, storage.hash_all()?);
    group.with_group_snapshot(|mls_group| {
        assert_eq!(mls_group.epoch().as_u64(), start_epoch);
        assert!(mls_group.pending_commit().is_none());
        Ok(())
    })?;
}
