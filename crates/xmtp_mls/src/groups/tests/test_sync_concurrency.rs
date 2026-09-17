//! Concurrent sync, cursors, and fork detection.

use super::*;

#[xmtp_common::test(unwrap_try = true)]
async fn process_messages_abort_on_retryable_error() {
    use crate::groups::mls_sync::GroupHeadOutcome;
    use xmtp_db::incoming_envelope::{IncomingRetry, QueryIncomingEnvelope, StreamTopic};
    use xmtp_proto::types::{OrderedEnvelopeBatch, Topic};

    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
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
            envelopes,
        },
        bo.context
            .incoming_runtime()
            .policy()
            .incoming_limits(topic.kind),
    )?;
    let db = bo.context.store().db();
    let received = db.topic_progress(&topic)?.received;
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
}

#[xmtp_common::test(unwrap_try = true)]
async fn skip_already_processed_messages() {
    use crate::groups::mls_sync::GroupHeadOutcome;
    use xmtp_db::incoming_envelope::{QueryIncomingEnvelope, StreamTopic};
    use xmtp_proto::types::{OrderedEnvelopeBatch, Topic};

    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
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
            envelopes,
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
    let alix = ClientBuilder::new_test_client_vanilla(&generate_local_wallet()).await;

    let bo_wallet = generate_local_wallet();
    let bo_client = ClientBuilder::new_test_client_vanilla(&bo_wallet).await;

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

#[xmtp_common::test(flavor = "multi_thread")]
async fn test_parallel_syncs() {
    tester!(alix1, sync_worker);

    let alix1_group = alix1.create_group(None, None).unwrap();

    tester!(alix2, from: alix1);

    let sync_tasks: Vec<_> = (0..10)
        .map(|_| {
            let group_clone = alix1_group.clone();
            // Each of these syncs is going to trigger the client to invite alix2 to the group
            // because of the race
            xmtp_common::spawn(None, async move { group_clone.sync().await }).join()
        })
        .collect();

    let results = join_all(sync_tasks).await;

    // Check if any of the syncs failed
    for result in results.into_iter() {
        assert!(result.is_ok(), "Sync error {:?}", result.err());
    }

    // Make sure that only one welcome was sent
    let alix2_welcomes = alix1
        .context
        .api()
        .query_welcome_messages(alix2.installation_public_key())
        .await
        .unwrap();
    assert_eq!(alix2_welcomes.len(), 1);

    // The installation add emits an Add proposal, a membership AppDataUpdate proposal,
    // and the commit that consumes both proposals.
    let group_messages = alix1
        .context
        .api()
        .query_group_messages(alix1_group.group_id)
        .await
        .unwrap();
    assert_eq!(group_messages.len(), 3);

    let alix2_group = receive_group_invite(&alix2).await;

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

#[rstest::rstest]
#[xmtp_common::test(flavor = "multi_thread")]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn add_missing_installs_reentrancy() {
    tester!(alix1);
    let alix1_group = alix1.create_group(None, None).unwrap();

    tester!(alix2, from: alix1);

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
#[xmtp_common::test(unwrap_try = true, flavor = "multi_thread")]
async fn test_when_processing_message_return_future_wrong_epoch_group_marked_probably_forked() {
    use crate::utils::test_mocks_helpers::set_test_mode_future_wrong_epoch;

    tester!(client_a);
    tester!(client_b);

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

#[xmtp_common::test(flavor = "multi_thread")]
async fn can_stream_out_of_order_without_forking() {
    tester!(client_a1);
    tester!(client_b);
    tester!(client_c);

    // Create a group
    let group_a = client_a1.create_group(None, None).unwrap();

    // Add client_b and client_c to the group
    group_a
        .add_members(&[client_b.inbox_id(), client_c.inbox_id()])
        .await
        .unwrap();

    // Sync the group
    client_b.sync_welcomes().await.unwrap();
    let binding = client_b.find_groups(GroupQueryArgs::default()).unwrap();
    let group_b = binding.first().unwrap();

    client_c.sync_welcomes().await.unwrap();
    let binding = client_c.find_groups(GroupQueryArgs::default()).unwrap();
    let group_c = binding.first().unwrap();

    // Each client sends a message and syncs (ensures any key update commits are sent)
    group_a
        .send_message_optimistic("Message a1".as_bytes(), SendMessageOpts::default())
        .unwrap();
    group_a.publish_intents().await.unwrap();

    group_a.sync().await.unwrap();
    group_b.sync().await.unwrap();
    group_c.sync().await.unwrap();

    group_b
        .send_message_optimistic("Message b1".as_bytes(), SendMessageOpts::default())
        .unwrap();
    group_b.publish_intents().await.unwrap();

    group_a.sync().await.unwrap();
    group_b.sync().await.unwrap();
    group_c.sync().await.unwrap();

    group_c
        .send_message_optimistic("Message c1".as_bytes(), SendMessageOpts::default())
        .unwrap();
    group_c.publish_intents().await.unwrap();

    // Sync the groups
    group_a.sync().await.unwrap();
    group_b.sync().await.unwrap();
    group_c.sync().await.unwrap();

    // After client a adds b and c, and they each sent a message, all groups are in the same epoch
    assert_eq!(group_a.epoch().await.unwrap(), 3);
    assert_eq!(group_b.epoch().await.unwrap(), 3);
    assert_eq!(group_c.epoch().await.unwrap(), 3);

    // Client b updates the group name, (incrementing the epoch from 3 to 4), and syncs
    group_b
        .update_group_name("Group B".to_string())
        .await
        .unwrap();
    group_b.sync().await.unwrap();

    // Client c sends two text messages before incrementing the epoch
    group_c
        .send_message_optimistic("Message c2".as_bytes(), SendMessageOpts::default())
        .unwrap();
    group_c.publish_intents().await.unwrap();
    group_b.sync().await.unwrap();

    // Retrieve all messages from group B, verify they contain the two messages from client c even though they were sent from the wrong epoch
    let messages = client_b
        .context
        .api()
        .query_group_messages(group_b.group_id)
        .await
        .unwrap();
    // Adding B and C emits two Add proposals, one membership AppDataUpdate proposal,
    // and their commit. The remaining eight envelopes are the messages and later commits.
    assert_eq!(messages.len(), 12);

    // Get reference to last message
    let last_message = messages.last().unwrap();

    // A notification fetches and processes its complete ordered prefix.
    let wire = group_a
        .context
        .api()
        .query_all(
            [(
                xmtp_proto::types::Topic::new_group_message(group_a.group_id),
                Cursor(0),
            )]
            .into(),
            group_a.context.api().limits().max_query_limit as u32,
        )
        .await
        .unwrap()
        .into_iter()
        .find(|envelope| {
            envelope
                .meta
                .as_ref()
                .and_then(|meta| meta.cursor.as_ref())
                .is_some_and(|cursor| cursor.sequence_id == last_message.cursor.0)
        })
        .unwrap();
    let result = group_a
        .process_streamed_group_message(wire.encode_to_vec())
        .await;
    assert!(result.is_ok());

    // All three installations must retain the intervening group-name commit.
    group_a.sync().await.unwrap();
    group_b.sync().await.unwrap();
    group_c.sync().await.unwrap();

    assert_eq!(group_b.epoch().await.unwrap(), 4);
    assert_eq!(group_c.epoch().await.unwrap(), 4);
    assert_eq!(group_a.epoch().await.unwrap(), 4);
}

#[xmtp_common::test(unwrap_try = true)]
async fn own_message_without_intent_skips_and_increments_cursor() {
    use crate::state_tx::state_write;
    use xmtp_db::TransactionOutcome::Continue;
    use xmtp_db::incoming_envelope::{QueryIncomingEnvelope, StreamTopic};

    tester!(alice, disable_workers);
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
async fn prepared_commit_keeps_keys_without_advancing_epoch() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let group = alix.create_group(None, None)?;
    group.add_members(&[bo.inbox_id()]).await?;
    group.sync().await?;

    let storage = alix.context.mls_storage();
    let start_hash = storage.hash_all()?;
    let start_epoch = group.epoch().await?;
    let installation_keys = &group.context.identity().installation_keys;
    crate::state_tx::state_write(storage, |tx| {
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
