//! Commit log fork detection tests: detection.

use super::*;

// A conversation whose log is not read has no remote entry to compare, so no
// fork check selects it, however many unsolicited conversations there are. It
// is checked once it is allowed.
// verifies: FORK-082
#[xmtp_common::test(unwrap_try = true)]
async fn fork_checks_select_only_allowed_conversations() {
    use xmtp_db::{
        consent_record::{ConsentState, ConsentType, StoredConsentRecord},
        group::{ConversationType, GroupMembershipState, StoredGroup},
        refresh_state::EntityKind,
    };
    use xmtp_proto::types::GroupId;

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let unsolicited_dm = bo.find_or_create_dm(alix.inbox_id(), None).await?;
    alix.sync_welcomes().await?;
    let incoming_dm = alix.group(&unsolicited_dm.group_id)?;
    assert_eq!(incoming_dm.consent_state()?, ConsentState::Unknown);

    // Unsolicited DM rows, half with no consent record and half denied.
    let db = alix.context.db();
    let mut unconsented = vec![incoming_dm.group_id];
    for marker in 100..164_u8 {
        let group_id = GroupId::from([marker; 32]);
        StoredGroup::builder()
            .id(group_id)
            .created_at_ns(i64::from(marker))
            .membership_state(GroupMembershipState::Allowed)
            .added_by_inbox_id("unsolicited-sender")
            .conversation_type(ConversationType::Dm)
            .dm_id(Some(format!("dm:unsolicited-sender:recipient-{marker}")))
            .build()?
            .store(&db)?;
        if marker % 2 == 0 {
            StoredConsentRecord {
                entity_type: ConsentType::ConversationId,
                entity: hex::encode(group_id),
                state: ConsentState::Denied,
                consented_at_ns: i64::from(marker),
                owed_seq: 0,
            }
            .store(&db)?;
        }
        unconsented.push(group_id);
    }
    let selected = db.get_conversation_ids_for_fork_check()?;
    assert_eq!(
        unconsented
            .iter()
            .filter(|id| selected.iter().any(|candidate| candidate == id.as_slice()))
            .count(),
        0,
        "no unknown or denied conversation is selected"
    );

    // The DM's Welcome entry moves its fork-check cursor only when a pass
    // checks it.
    let checked_through = || {
        db.get_last_cursor(incoming_dm.group_id, EntityKind::CommitLogForkCheckLocal)
            .unwrap()
    };
    let mut worker = CommitLogWorker::new(alix.context.clone());
    worker.update_forked_state().await?;
    let before_allowed = checked_through();
    assert_eq!(before_allowed, Cursor(0));
    incoming_dm.update_consent_state(ConsentState::Allowed)?;
    worker.update_forked_state().await?;
    assert!(checked_through() > before_allowed);
}

// The fault is artificial. State changes, log writing, signatures, transport,
// remote verification, and fork detection use the normal implementation.
// verifies: FORK-082, FORK-073
#[xmtp_common::test(unwrap_try = true)]
async fn test_real_divergence_detected_through_signed_commit_logs() {
    use crate::groups::test_mocks::set_test_mode_future_wrong_epoch;
    use xmtp_db::consent_record::ConsentState;

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let a = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let b = bo.wait_for_welcomes().await?.first()?.to_owned();
    b.update_consent_state(ConsentState::Allowed)?;

    // Start with a shared successful commit after Bo's Welcome anchor.
    a.update_group_name("shared".into()).await?;
    let target = Cursor(a.local_commit_log().await?.last()?.commit_sequence_id as u64);
    crate::subscriptions::barrier::wait_through(
        &b.context,
        [(Topic::new_group_message(b.group_id), target)].into(),
        None,
    )
    .await?;
    assert_eq!(
        b.context
            .db()
            .topic_progress(&StreamTopic::group(b.group_id))?
            .processed,
        target
    );
    assert_eq!(
        a.context
            .db()
            .topic_progress(&StreamTopic::group(a.group_id))?
            .processed,
        target
    );
    let shared_epoch = b.epoch().await?;
    let shared_authenticator = b.epoch_authenticator().await?;
    assert_eq!(a.epoch().await?, shared_epoch);
    assert_eq!(a.epoch_authenticator().await?, shared_authenticator);

    a.update_group_name("diverged".into()).await?;
    // Reject one valid commit on Bo. Always remove the injected fault before
    // checking the result, so subsequent delivery uses real epoch validation.
    let target = Cursor(a.local_commit_log().await?.last()?.commit_sequence_id as u64);
    set_test_mode_future_wrong_epoch(true);
    let rejected_sync = crate::subscriptions::barrier::wait_through(
        &b.context,
        [(Topic::new_group_message(b.group_id), target)].into(),
        None,
    )
    .await;
    set_test_mode_future_wrong_epoch(false);
    rejected_sync?;
    assert_eq!(b.epoch().await?, shared_epoch);
    assert_eq!(b.epoch_authenticator().await?, shared_authenticator);
    assert!(a.epoch().await? > b.epoch().await?);
    assert_ne!(
        a.epoch_authenticator().await?,
        b.epoch_authenticator().await?
    );

    let rejected_logs = b.local_commit_log().await?;
    let rejected = rejected_logs.last()?;
    assert_eq!(rejected.commit_result, CommitResult::WrongEpoch);
    assert_eq!(rejected.last_epoch_authenticator, shared_authenticator);
    assert_eq!(rejected.applied_epoch_authenticator, shared_authenticator);
    let applied_logs = a.local_commit_log().await?;
    let applied = applied_logs.last()?;
    assert_eq!(applied.commit_sequence_id, rejected.commit_sequence_id);
    assert_eq!(applied.commit_result, CommitResult::Success);
    assert_eq!(
        applied.applied_epoch_authenticator,
        a.epoch_authenticator().await?
    );
    assert_ne!(
        applied.applied_epoch_authenticator,
        rejected.applied_epoch_authenticator
    );

    // Clear only the synthetic fault's flag inside this test. A later commit
    // now has a genuinely future epoch for Bo and must set the flag itself.
    bo.context.db().clear_fork_flag_for_group(&b.group_id)?;
    assert!(!b.debug_info().await?.maybe_forked);
    a.update_group_name("future epoch".into()).await?;
    let target = Cursor(a.local_commit_log().await?.last()?.commit_sequence_id as u64);
    crate::subscriptions::barrier::wait_through(
        &b.context,
        [(Topic::new_group_message(b.group_id), target)].into(),
        None,
    )
    .await?;
    // Both clients processed the same prefix, but their MLS states differ.
    for group in [&a, &b] {
        assert_eq!(
            group
                .context
                .db()
                .topic_progress(&StreamTopic::group(group.group_id))?
                .processed,
            target
        );
    }
    assert!(b.debug_info().await?.maybe_forked);
    assert_eq!(b.epoch().await?, shared_epoch);
    assert_eq!(b.epoch_authenticator().await?, shared_authenticator);
    assert_ne!(
        a.epoch_authenticator().await?,
        b.epoch_authenticator().await?
    );

    // Local divergence alone is not remote verification. Missing signed
    // evidence must remain unknown until the publisher's records arrive.
    let mut checker = CommitLogWorker::new(bo.context.clone());
    let unknown = checker
        .run_test(CommitLogTestFunction::CheckForkedState, None)
        .await?;
    assert_eq!(unknown[0].is_forked.as_ref()?[b.group_id.as_slice()], None);

    let mut publisher = CommitLogWorker::new(alix.context.clone());
    publisher
        .run_test(CommitLogTestFunction::PublishCommitLogsToRemote, None)
        .await?;
    let saved = checker
        .run_test(CommitLogTestFunction::SaveRemoteCommitLog, None)
        .await?;
    assert!(saved[0].save_remote_commit_log_results.as_ref()?[b.group_id.as_slice()] > 0);
    let checked = checker
        .run_test(CommitLogTestFunction::CheckForkedState, None)
        .await?;
    assert_eq!(
        checked[0].is_forked.as_ref()?[b.group_id.as_slice()],
        Some(true)
    );
    assert_eq!(
        bo.context
            .db()
            .get_group_commit_log_forked_status(&b.group_id)?,
        Some(true)
    );
}

// verifies: EVENT-001, EVENT-010
#[xmtp_common::test(unwrap_try = true)]
async fn test_commit_log_fork_detection_forked() -> Result<(), Box<dyn std::error::Error>> {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let group = alix.find_or_create_dm(bo.inbox_id(), None).await?;
    let group_id = group.group_id;

    // Insert local commit log entries
    let local_entry_1 = NewLocalCommitLog {
        group_id,
        commit_sequence_id: 200,
        last_epoch_authenticator: vec![0x11, 0x22, 0x33],
        commit_result: CommitResult::Success,
        error_message: None,
        applied_epoch_number: 1,
        applied_epoch_authenticator: vec![0xAA, 0xBB, 0xCC],
        sender_inbox_id: None,
        sender_installation_id: None,
        commit_type: None,
    };

    let local_entry_2 = NewLocalCommitLog {
        group_id,
        commit_sequence_id: 201,
        last_epoch_authenticator: vec![0xAA, 0xBB, 0xCC],
        commit_result: CommitResult::Success,
        error_message: None,
        applied_epoch_number: 2,
        applied_epoch_authenticator: vec![0xDD, 0xEE, 0xFF],
        sender_inbox_id: None,
        sender_installation_id: None,
        commit_type: None,
    };

    local_entry_1.store(&alix.context.db())?;
    local_entry_2.store(&alix.context.db())?;

    // Insert matching remote commit log entries (no fork)
    let remote_entry_1 = NewRemoteCommitLog {
        log_sequence_id: 100,
        group_id,
        commit_sequence_id: 200,
        commit_result: CommitResult::Invalid, // For some reason remote marked this commit invalid
        applied_epoch_number: 1,
        applied_epoch_authenticator: vec![0xAA, 0xBB, 0xCC], // Same as local
        author_installation_key: vec![],
    };

    let remote_entry_2 = NewRemoteCommitLog {
        log_sequence_id: 101,
        group_id,
        commit_sequence_id: 200,
        commit_result: CommitResult::Success,
        applied_epoch_number: 1,
        applied_epoch_authenticator: vec![0xCD, 0xDE, 0xEF], // Different from local
        author_installation_key: vec![],
    };

    remote_entry_1.store(&alix.context.db())?;
    remote_entry_2.store(&alix.context.db())?;

    // Test fork detection
    let events = alix.context.events().subscribe(
        xmtp_events::EventFilter::new([xmtp_events::EventKind::ConversationForkDetected]),
        Some(4),
    );
    let mut worker = CommitLogWorker::new(alix.context.clone());
    let results = worker
        .run_test(CommitLogTestFunction::CheckForkedState, None)
        .await?;

    // Should detect a fork
    assert_eq!(results.len(), 1);
    let result = &results[0];
    assert!(result.is_forked.is_some());
    let fork_status = result
        .is_forked
        .as_ref()
        .unwrap()
        .get(group_id.as_ref())
        .unwrap();
    assert_eq!(*fork_status, Some(true), "Should detect a fork");
    worker
        .run_test(CommitLogTestFunction::CheckForkedState, None)
        .await?;
    let envelopes = events.drain();
    assert!(matches!(
        envelopes.as_slice(),
        [xmtp_events::EventEnvelope {
            client: Some(xmtp_events::ClientEvent::ConversationForkDetected(fork)), ..
        }] if fork.group_id == group_id.to_vec()
    ));
    assert_eq!(
        envelopes[0].context.dm_identifier,
        Some(group.dm_id.clone().expect("DM ID").into_bytes())
    );

    Ok(())
}
