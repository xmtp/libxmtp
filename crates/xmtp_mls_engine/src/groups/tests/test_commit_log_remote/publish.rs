//! Remote commit log tests: publish.

use super::*;

/// A conversation's local entries that have a commit sequence id: the ones
/// its worker publishes.
async fn publishable(group: &MlsGroup<impl XmtpSharedContext>) -> Vec<PlaintextCommitLogEntry> {
    group
        .local_commit_log()
        .await
        .unwrap()
        .iter()
        .filter(|log| log.commit_sequence_id > 0)
        .map(|log| {
            let mut entry = PlaintextCommitLogEntry::from(log);
            sign_entry_author(
                &keys_of(group),
                &group.context.identity().installation_keys,
                &mut entry,
            )
            .unwrap();
            entry
        })
        .collect()
}

fn upload_cursor(client: &EngineTestPeer, group_id: &GroupId) -> Cursor {
    client
        .context
        .db()
        .get_last_cursor(group_id, EntityKind::CommitLogUpload)
        .unwrap()
}

/// A restored placeholder holds no root and no commit-log id. The Welcome
/// that activates it stores the commit-log id its root derives.
#[xmtp_common::test(unwrap_try = true)]
async fn activating_a_placeholder_stores_its_commit_log_id() {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;

    let a = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    // Bo holds the group only as an archive placeholder.
    MlsGroup::insert(
        &bo.context,
        Some(a.group_id.as_slice()),
        GroupMembershipState::Restored,
        ConversationType::Group,
        PolicySet::default(),
        GroupMetadataOptions::default(),
        None,
        false,
    )?;
    let placeholder = bo.context.db().find_group(&a.group_id)??;
    assert_eq!(placeholder.commit_log_public_key, None);

    bo.sync_welcomes().await?;
    let activated = bo.context.db().find_group(&a.group_id)??;
    assert_ne!(activated.membership_state, GroupMembershipState::Restored);
    assert_eq!(
        activated.commit_log_public_key,
        Some(keys_of(&a).commit_log_id().to_vec())
    );
}

/// The worker publishes each local entry as a sealed entry on the topic its
/// commit-log id names, not its group id. The envelope holds neither the
/// group id nor the entry's authenticators, the entry opens to the local
/// one, and sealing it again gives the same bytes, which the backend stores
/// once.
// verifies: FORK-074
#[xmtp_common::test(unwrap_try = true)]
async fn test_publish_commit_log_to_remote() {
    // Disable background CommitLogWorker for deterministic testing
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;

    // Alix creates a group with Bo
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;
    bo.sync_all_welcomes_and_groups(None).await?;

    // Alix has two local commit log entries; only the one with a commit
    // sequence id is published.
    let commit_log_entries = alix.context.db().get_group_logs(&alix_group.group_id)?;
    assert_eq!(commit_log_entries.len(), 2);
    assert_eq!(upload_cursor(&alix, &alix_group.group_id), Cursor(0));

    let mut commit_log_worker = CommitLogWorker::new(alix.context.clone());
    commit_log_worker
        .run_test(CommitLogTestFunction::PublishCommitLogsToRemote, Some(1))
        .await?;
    let last_commit_log_entry = commit_log_entries.last()?;
    assert_eq!(
        upload_cursor(&alix, &alix_group.group_id),
        Cursor(last_commit_log_entry.rowid as u64)
    );

    let keys = keys_of(&alix_group);
    let topic = Topic::new_commit_log(keys.commit_log_id());
    assert_ne!(topic.identifier(), alix_group.group_id.as_slice());
    let response = alix
        .context
        .api()
        .query_commit_log([(topic.clone(), Cursor(0))].into())
        .await?;
    assert_eq!(response.len(), 1);
    let sealed = response[0].sealed.clone()?;
    let entry = open_entry(
        &keys,
        alix_group.group_id.as_slice(),
        response[0].cursor.0,
        &sealed,
    )?;
    let mut expected = PlaintextCommitLogEntry::from(last_commit_log_entry);
    sign_entry_author(
        &keys,
        &alix.context.identity().installation_keys,
        &mut expected,
    )?;
    assert_eq!(entry, expected);

    let envelope = wire::ClientEnvelope {
        payload: Some(Payload::SealedCommitLogEntry(sealed.clone())),
    }
    .encode_to_vec();
    assert!(!contains(&envelope, alix_group.group_id.as_slice()));
    assert!(!contains(&envelope, &entry.last_epoch_authenticator));
    assert!(!contains(&envelope, &entry.applied_epoch_authenticator));

    // Sealing the entry again gives the same bytes, so a retry is a
    // duplicate: the backend returns the stored envelope's position.
    assert_eq!(seal_entry(&keys, &entry)?, sealed);
    let mut stored = Vec::new();
    alix.context
        .api()
        .publish_commit_log(vec![sealed], &mut stored)
        .await?;
    assert_eq!(stored[0].cursor.as_ref()?.sequence_id, response[0].cursor.0);
    let again = alix
        .context
        .api()
        .query_commit_log([(topic, Cursor(0))].into())
        .await?;
    assert_eq!(again.len(), 1);
}

/// A rate-limited publish is not retried at the next turn inside its delay.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn a_rate_limited_publish_waits_out_its_delay() {
    let alix = EngineTestPeer::new().await;
    let group = alix.create_group(None, None)?;
    group.update_group_name("limited".into()).await?;
    let (client, script) = scripted::scripted(&alix).await;
    script.lock().rate_limit = Some(xmtp_common::time::Duration::from_secs(300));
    let id = keys_of(&group).commit_log_id().to_vec();
    let mut worker = CommitLogWorker::new(client.context.clone());
    assert!(publish_turn(&mut worker).await.is_empty());
    assert_eq!(script.lock().publishes_for(&id), 1);
    assert!(publish_turn(&mut worker).await.is_empty());
    assert_eq!(
        script.lock().publishes_for(&id),
        1,
        "no publish inside the delay"
    );
}

/// A storage reconnect builds a new worker from the same factory, which keeps
/// the publish hold: the new worker does not publish inside the delay.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn a_publish_hold_survives_a_new_worker() {
    let alix = EngineTestPeer::new().await;
    let group = alix.create_group(None, None)?;
    group.update_group_name("limited".into()).await?;
    let (client, script) = scripted::scripted(&alix).await;
    script.lock().rate_limit = Some(Duration::from_secs(300));
    let id = keys_of(&group).commit_log_id().to_vec();
    let factory = Factory::new(client.context.clone());
    assert!(publish_turn(&mut factory.worker()).await.is_empty());
    assert_eq!(script.lock().publishes_for(&id), 1);
    assert!(publish_turn(&mut factory.worker()).await.is_empty());
    assert_eq!(
        script.lock().publishes_for(&id),
        1,
        "no publish inside the delay"
    );
}

/// A readd answer refused with a rate limit keeps the request and is not
/// answered again inside the delay, also by a worker a storage reconnect
/// builds from the same factory.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn a_rate_limited_readd_waits_out_its_delay() {
    let alix = EngineTestPeer::new_with(|o| o.enable_fork_recovery_requests()).await;
    let bo = EngineTestPeer::new().await;
    let dm = alix
        .find_or_create_dm(bo.inbox_id().to_string(), None)
        .await?;
    bo.sync_all_welcomes_and_groups(None).await?;
    let bo_dm = bo.group(&dm.group_id)?;
    bo_dm.update_consent_state(ConsentState::Allowed)?;
    dm.key_update().await?;
    bo_dm.sync().await?;
    let mut alix_worker = CommitLogWorker::new(alix.context.clone());
    let (bo_client, script) = scripted::scripted(&bo).await;
    let factory = Factory::new(bo_client.context.clone());
    let mut bo_worker = factory.worker();
    alix_worker._tick().await?;
    bo_worker._tick().await?;
    alix.context
        .db()
        .set_group_commit_log_forked_status(&dm.group_id, Some(true))?;
    alix_worker._tick().await?;
    bo.sync_welcomes().await?;
    let installation = alix.context.installation_id();
    let bo_db = bo.context.db();
    assert!(bo_db.is_awaiting_readd(&dm.group_id, installation.as_slice())?);

    script.lock().group_message_rate_limit = Some(Duration::from_secs(300));
    let epoch = bo_dm.epoch().await?;
    bo_worker._tick().await?;
    let attempts = script.lock().group_message_publishes;
    assert!(attempts > 0, "the readd is sent");
    assert_eq!(bo_dm.epoch().await?, epoch);
    assert!(bo_db.is_awaiting_readd(&dm.group_id, installation.as_slice())?);
    bo_worker._tick().await?;
    factory.worker()._tick().await?;
    assert_eq!(
        script.lock().group_message_publishes,
        attempts,
        "no readd inside the delay"
    );
    assert!(bo_db.is_awaiting_readd(&dm.group_id, installation.as_slice())?);
}

/// A rate-limited conversation holds only its own commit-log publish: another
/// conversation still publishes inside the delay.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn a_rate_limited_publish_holds_only_its_conversation() {
    let alix = EngineTestPeer::new().await;
    let first = alix.create_group(None, None)?;
    first.update_group_name("one".into()).await?;
    let second = alix.create_group(None, None)?;
    second.update_group_name("two".into()).await?;
    let ids = [
        keys_of(&first).commit_log_id().to_vec(),
        keys_of(&second).commit_log_id().to_vec(),
    ];
    let (client, script) = scripted::scripted(&alix).await;
    script.lock().rate_limit = Some(xmtp_common::time::Duration::from_secs(300));
    let mut worker = CommitLogWorker::new(client.context.clone());
    assert!(publish_turn(&mut worker).await.is_empty());
    let refused = ids
        .iter()
        .find(|id| script.lock().publishes_for(id) == 1)?
        .clone();
    let other = ids.iter().find(|id| **id != refused)?.clone();
    script.lock().rate_limit = None;
    publish_turn(&mut worker).await;
    assert_eq!(
        script.lock().publishes_for(&refused),
        1,
        "no publish of the refused conversation inside its delay"
    );
    assert!(
        script.lock().publishes_for(&other) >= 1,
        "the other conversation still publishes"
    );
}

/// A rate limit on one conversation's publish does not end the turn: the
/// next conversation still publishes in the same turn.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn a_rate_limited_publish_does_not_end_the_turn() {
    let alix = EngineTestPeer::new().await;
    let first = alix.create_group(None, None)?;
    first.update_group_name("one".into()).await?;
    let second = alix.create_group(None, None)?;
    second.update_group_name("two".into()).await?;
    let ids = [
        keys_of(&first).commit_log_id().to_vec(),
        keys_of(&second).commit_log_id().to_vec(),
    ];
    let (client, script) = scripted::scripted(&alix).await;
    {
        let mut script = script.lock();
        script.rate_limit = Some(xmtp_common::time::Duration::from_secs(300));
        script.rate_limit_once = true;
    }
    let mut worker = CommitLogWorker::new(client.context.clone());
    let published = publish_turn(&mut worker).await;
    for id in &ids {
        assert_eq!(
            script.lock().publishes_for(id),
            1,
            "each conversation is sent once"
        );
    }
    assert_eq!(
        published.len(),
        1,
        "the conversation after the refused one publishes"
    );
}

/// Publication resumes once a rate-limit hold has ended.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn a_commit_log_publish_resumes_after_its_hold() {
    let alix = EngineTestPeer::new().await;
    let group = alix.create_group(None, None)?;
    group.update_group_name("limited".into()).await?;
    let (client, script) = scripted::scripted(&alix).await;
    script.lock().rate_limit = Some(xmtp_common::time::Duration::from_millis(50));
    let mut worker = CommitLogWorker::new(client.context.clone());
    assert!(publish_turn(&mut worker).await.is_empty());
    script.lock().rate_limit = None;
    xmtp_common::time::sleep(xmtp_common::time::Duration::from_millis(60)).await;
    assert_eq!(
        publish_turn(&mut worker).await,
        vec![group.group_id.to_vec()]
    );
}

/// The backend refuses one conversation's entries. Another conversation's
/// entries still publish in that turn and in the next, the refused
/// conversation keeps its entries pending, and once it is accepted it
/// publishes them from its first entry.
// verifies: FORK-021
#[xmtp_common::test(unwrap_try = true)]
async fn a_refused_conversation_does_not_hold_back_another() {
    let alix = EngineTestPeer::new().await;
    let refused = alix.create_group(None, None)?;
    let accepted = alix.create_group(None, None)?;
    refused.update_group_name("refused".into()).await?;
    accepted.update_group_name("accepted".into()).await?;

    let (client, script) = scripted::scripted(&alix).await;
    let refused_id = keys_of(&refused).commit_log_id().to_vec();
    script.lock().refuse.insert(refused_id.clone());
    let mut worker = CommitLogWorker::new(client.context.clone());

    for turn in 1..=2 {
        if turn == 2 {
            accepted.update_group_name("later".into()).await?;
        }
        assert_eq!(
            publish_turn(&mut worker).await,
            vec![accepted.group_id.to_vec()]
        );
        assert_eq!(worker.failed_publishes(), &[refused.group_id]);
        assert_eq!(
            upload_cursor(&alix, &refused.group_id),
            Cursor(0),
            "the refused entries stay pending"
        );
        let tip = accepted.local_commit_log().await?.last()?.rowid;
        assert_eq!(upload_cursor(&alix, &accepted.group_id), Cursor(tip as u64));
        assert_eq!(script.lock().publishes_for(&refused_id), turn);
    }
    assert_eq!(
        remote_entries(&alix, &accepted).await,
        publishable(&accepted).await
    );

    // Once the backend accepts it, the held conversation publishes from its
    // first entry.
    script.lock().refuse.clear();
    assert_eq!(
        publish_turn(&mut worker).await,
        vec![refused.group_id.to_vec()]
    );
    assert!(worker.failed_publishes().is_empty());
    let tip = refused.local_commit_log().await?.last()?.rowid;
    assert_eq!(upload_cursor(&alix, &refused.group_id), Cursor(tip as u64));
    assert_eq!(
        remote_entries(&alix, &refused).await,
        publishable(&refused).await
    );
}

/// One conversation's entry fails the local admission parse: its nonce is a
/// byte short. Nothing of that conversation is sent, and the other
/// conversation still publishes in the same turn and in the next one.
// verifies: FORK-021
#[xmtp_common::test(unwrap_try = true)]
async fn a_conversation_that_fails_the_local_parse_does_not_hold_back_another() {
    let alix = EngineTestPeer::new().await;
    let broken = alix.create_group(None, None)?;
    let healthy = alix.create_group(None, None)?;
    broken.update_group_name("broken".into()).await?;
    healthy.update_group_name("healthy".into()).await?;

    let (client, script) = scripted::scripted(&alix).await;
    let broken_group = broken.group_id;
    let mut worker = CommitLogWorker::new(client.context.clone()).with_seal_fault(Arc::new(
        move |group_id, sealed| {
            let mut sealed = sealed?;
            if *group_id == broken_group {
                sealed.nonce.pop();
            }
            Ok(sealed)
        },
    ));

    for turn in 1..=2 {
        if turn == 2 {
            healthy.update_group_name("later".into()).await?;
        }
        assert_eq!(
            publish_turn(&mut worker).await,
            vec![healthy.group_id.to_vec()]
        );
        assert_eq!(worker.failed_publishes(), &[broken.group_id]);
        assert_eq!(upload_cursor(&alix, &broken.group_id), Cursor(0));
        assert_eq!(
            script
                .lock()
                .publishes_for(&keys_of(&broken).commit_log_id()),
            0,
            "the local parse refuses the entry before any request"
        );
        let tip = healthy.local_commit_log().await?.last()?.rowid;
        assert_eq!(upload_cursor(&alix, &healthy.group_id), Cursor(tip as u64));
        assert_eq!(
            remote_entries(&alix, &healthy).await,
            publishable(&healthy).await
        );
    }
}

/// A conversation whose stored group state does not load holds back only
/// itself: the other conversation still publishes in the same turn, and the
/// turn does not fail.
// verifies: FORK-021
#[xmtp_common::test(unwrap_try = true)]
async fn a_conversation_whose_state_does_not_load_does_not_hold_back_another() {
    use openmls_traits::storage::StorageProvider as _;

    let alix = EngineTestPeer::new().await;
    let broken = alix.create_group(None, None)?;
    let healthy = alix.create_group(None, None)?;
    broken.update_group_name("broken".into()).await?;
    healthy.update_group_name("healthy".into()).await?;
    let broken_id = keys_of(&broken).commit_log_id().to_vec();
    alix.context
        .mls_storage()
        .delete_context(&openmls::group::GroupId::from_slice(
            broken.group_id.as_slice(),
        ))?;

    let (client, script) = scripted::scripted(&alix).await;
    let mut worker = CommitLogWorker::new(client.context.clone());
    assert_eq!(
        publish_turn(&mut worker).await,
        vec![healthy.group_id.to_vec()]
    );
    assert_eq!(upload_cursor(&alix, &broken.group_id), Cursor(0));
    assert_eq!(script.lock().publishes_for(&broken_id), 0);
    assert_eq!(
        remote_entries(&alix, &healthy).await,
        publishable(&healthy).await
    );
}

/// A transport failure ends the publishing part of the turn. Once the first
/// conversation's publish has failed through its retries, no other
/// conversation is tried in that turn, so a turn costs one retry cycle, not
/// one per conversation. The next turn publishes every conversation.
// verifies: FORK-021
#[xmtp_common::test(unwrap_try = true)]
async fn a_transport_failure_ends_the_publishing_turn() {
    let alix = EngineTestPeer::new().await;
    let first = alix.create_group(None, None)?;
    let second = alix.create_group(None, None)?;
    first.update_group_name("first".into()).await?;
    second.update_group_name("second".into()).await?;

    let (client, script) = scripted::scripted(&alix).await;
    script.lock().unavailable = true;
    let mut worker = CommitLogWorker::new(client.context.clone());
    assert!(publish_turn(&mut worker).await.is_empty());
    let attempted: std::collections::HashSet<Vec<u8>> =
        script.lock().published.iter().flatten().cloned().collect();
    assert_eq!(attempted.len(), 1, "one conversation is tried");
    assert_eq!(worker.failed_publishes().len(), 1);

    script.lock().unavailable = false;
    let mut published = publish_turn(&mut worker).await;
    published.sort();
    let mut expected = vec![first.group_id.to_vec(), second.group_id.to_vec()];
    expected.sort();
    assert_eq!(published, expected);
    assert!(worker.failed_publishes().is_empty());
}

/// A publish that stores only part of a conversation's backlog moves the
/// upload cursor past the stored entries. The next turn starts at the first
/// entry that was not stored. The backlog is split by a size refusal; the
/// first half is stored and the second half is refused.
// verifies: FORK-021
#[xmtp_common::test(unwrap_try = true)]
async fn a_partly_stored_publish_resumes_from_its_first_unstored_entry() {
    let alix = EngineTestPeer::new().await;
    let group = alix.create_group(None, None)?;
    for name in ["one", "two", "three"] {
        group.update_group_name(name.into()).await?;
    }
    let rows: Vec<i64> = group
        .local_commit_log()
        .await?
        .iter()
        .map(|log| log.rowid as i64)
        .collect();
    let count = rows.len();
    assert!(count >= 2, "{count} entries");

    let (client, script) = scripted::scripted(&alix).await;
    let commit_log_id = keys_of(&group).commit_log_id().to_vec();
    {
        let mut script = script.lock();
        script.too_large_once.insert(commit_log_id.clone());
        script.refuse_after.insert(commit_log_id.clone(), 1);
    }
    let mut worker = CommitLogWorker::new(client.context.clone());
    assert!(publish_turn(&mut worker).await.is_empty());
    let stored = count / 2;
    assert_eq!(
        upload_cursor(&alix, &group.group_id),
        Cursor(rows[stored - 1] as u64),
        "the cursor moves past the stored half only"
    );

    script.lock().refuse_after.clear();
    let sent_before = script.lock().published.len();
    assert_eq!(
        publish_turn(&mut worker).await,
        vec![group.group_id.to_vec()]
    );
    let requests = script.lock().published[sent_before..].to_vec();
    assert_eq!(
        requests.iter().map(Vec::len).collect::<Vec<_>>(),
        vec![count - stored],
        "the next turn sends only the unstored entries"
    );
    assert_eq!(
        upload_cursor(&alix, &group.group_id),
        Cursor(*rows.last()? as u64)
    );
    assert_eq!(
        remote_entries(&alix, &group).await,
        publishable(&group).await
    );
}

/// A seal that fails its self-check holds back only its own conversation for
/// the turn. None of its entries is sent, and the log names the failure but
/// none of the entry's sealed bytes. The next turn seals it again.
// verifies: FORK-074
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn a_seal_failure_skips_only_that_conversation() {
    use xmtp_logging::{Level, test_logging::LogCapture};

    let alix = EngineTestPeer::new().await;
    let faulty = alix.create_group(None, None)?;
    let healthy = alix.create_group(None, None)?;
    faulty.update_group_name("faulty".into()).await?;
    healthy.update_group_name("healthy".into()).await?;

    let (client, script) = scripted::scripted(&alix).await;
    let faulty_group = faulty.group_id;
    let failing = Arc::new(AtomicBool::new(true));
    let fault = failing.clone();
    let mut worker = CommitLogWorker::new(client.context.clone()).with_seal_fault(Arc::new(
        move |group_id, sealed| {
            if *group_id == faulty_group && fault.load(Ordering::SeqCst) {
                return Err(SealError::SelfCheck);
            }
            sealed
        },
    ));

    let capture = LogCapture::new(Level::Info);
    let published = {
        // The current-thread runtime polls spawned tasks on this thread too.
        let _logging = tracing::dispatcher::set_default(&capture.dispatch());
        publish_turn(&mut worker).await
    };
    assert_eq!(published, vec![healthy.group_id.to_vec()]);
    assert_eq!(upload_cursor(&alix, &faulty.group_id), Cursor(0));
    let faulty_keys = keys_of(&faulty);
    assert_eq!(script.lock().publishes_for(&faulty_keys.commit_log_id()), 0);
    let output = capture.output();
    assert!(output.contains("self-check failed"), "{output}");
    for entry in publishable(&faulty).await {
        let sealed = seal_entry(&faulty_keys, &entry)?;
        for bytes in [&sealed.signature, &sealed.nonce, &sealed.ciphertext] {
            assert!(!output.contains(&hex::encode(bytes)), "{output}");
        }
    }

    failing.store(false, Ordering::SeqCst);
    assert_eq!(
        publish_turn(&mut worker).await,
        vec![faulty.group_id.to_vec()]
    );
    let tip = faulty.local_commit_log().await?.last()?.rowid;
    assert_eq!(upload_cursor(&alix, &faulty.group_id), Cursor(tip as u64));
}

/// A failed publish ends the worker's turn, not the worker. The worker keeps
/// running, and it does not seal or send the refused entries again before
/// its next interval tick.
// verifies: FORK-021
#[xmtp_common::test(unwrap_try = true)]
async fn a_failed_publish_ends_the_turn_not_the_worker() {
    use futures::future::{Either, select};

    // The first tick runs at once on native and one interval after the start
    // on wasm, so the interval bounds both the wait for the first tick and
    // the room after it. The refused publish must be observed, and the window
    // after it must close, before the next tick.
    const INTERVAL: Duration = Duration::from_secs(10);
    const AFTER_PUBLISH: Duration = Duration::from_secs(2);
    let alix = EngineTestPeer::new_with(|o| {
        o.fork_recovery_opts = xmtp_mls_core::config::ForkRecoveryOpts {
            worker_interval_ns: Some(INTERVAL.as_nanos() as u64),
            ..Default::default()
        }
    })
    .await;
    let group = alix.create_group(None, None)?;
    group.update_group_name("refused".into()).await?;
    let pending = publishable(&group).await.len();
    assert!(pending > 0);

    let (client, script) = scripted::scripted(&alix).await;
    let commit_log_id = keys_of(&group).commit_log_id().to_vec();
    script.lock().refuse.insert(commit_log_id.clone());
    let seals = Arc::new(AtomicUsize::new(0));
    let counted = seals.clone();
    let mut worker =
        CommitLogWorker::new(client.context.clone()).with_seal_fault(Arc::new(move |_, sealed| {
            counted.fetch_add(1, Ordering::SeqCst);
            sealed
        }));

    let window = async {
        xmtp_common::time::timeout(INTERVAL * 2, async {
            while script.lock().publishes_for(&commit_log_id) == 0 {
                xmtp_common::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("the first tick publishes");
        xmtp_common::time::sleep(AFTER_PUBLISH).await;
    };
    let run = worker._run();
    futures::pin_mut!(window, run);
    if let Either::Right((stopped, _)) = select(window, run).await {
        panic!("the worker stopped: {stopped:?}");
    }
    assert_eq!(seals.load(Ordering::SeqCst), pending, "sealed once");
    assert_eq!(script.lock().publishes_for(&commit_log_id), 1, "sent once");
}
