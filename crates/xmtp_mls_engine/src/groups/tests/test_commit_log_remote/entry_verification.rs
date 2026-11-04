//! Remote commit log tests: entry verification.

use super::*;

// Helper functions for tracking commit types
fn get_commit_types_as_strings(logs: &[LocalCommitLog]) -> Vec<String> {
    logs.iter()
        .map(|l| l.commit_type.clone().unwrap_or_else(|| "None".to_string()))
        .collect()
}

async fn print_commit_log_with_types(group: &MlsGroup<impl XmtpSharedContext>) {
    let logs = group.local_commit_log().await.unwrap();
    println!(
        "Commit log for group {}: {:?}",
        hex::encode(&group.group_id.as_slice()[0..4]),
        get_commit_types_as_strings(&logs)
    );
}

fn assert_commit_sequence(logs: &[LocalCommitLog], expected: &[CommitType]) {
    let actual_types = get_commit_types_as_strings(logs);
    let expected_types: Vec<String> = expected.iter().map(|t| t.to_string()).collect();

    assert_eq!(
        actual_types, expected_types,
        "Commit sequence mismatch. Expected: {:?}, Got: {:?}",
        expected_types, actual_types
    );
}

fn root_of(group: &MlsGroup<impl XmtpSharedContext>) -> GroupRoot {
    read_group_root(group.load_group_context().unwrap().extensions())
        .unwrap()
        .unwrap()
}

/// The commit-log signing seed, derived from the root without `GroupKeys`, so
/// a test can sign an entry with any field lengths.
fn commit_log_seed(root: &GroupRoot) -> [u8; 32] {
    let hkdf = hkdf::Hkdf::<sha2::Sha256>::new(Some(b"xmtp group root"), root.as_bytes());
    let mut seed = [0; 32];
    hkdf.expand(b"xmtp commit log signing key", &mut seed)
        .unwrap();
    seed
}

/// A sealed entry with these fields, signed by `seed` over the
/// `CommitLogEntryTBS` of exactly these fields.
fn signed_entry(
    seed: &[u8; 32],
    commit_log_id: &[u8],
    nonce: Vec<u8>,
    ciphertext: Vec<u8>,
) -> SealedCommitLogEntry {
    use tls_codec::Serialize;
    let topic = Topic::new_commit_log(commit_log_id).cloned_vec();
    let mut tbs = Vec::new();
    for field in [
        b"xmtp commit log entry".as_slice(),
        &topic,
        &nonce,
        &ciphertext,
    ] {
        tls_codec::VLByteSlice(field)
            .tls_serialize(&mut tbs)
            .unwrap();
    }
    let signature = RustCrypto::default()
        .sign(SignatureScheme::ED25519, &tbs, seed)
        .unwrap();
    SealedCommitLogEntry {
        commit_log_id: commit_log_id.to_vec(),
        nonce,
        ciphertext,
        signature,
    }
}

/// The read path opens each envelope under the conversation's stored
/// commit-log id, with no first-entry selection. A valid entry sealed under
/// another root's commit-log id, an entry signed by another key, one that
/// does not decrypt, one that names another group, entries with a 0-, 11-,
/// or 13-byte nonce or a 15-byte ciphertext, and an envelope that holds no
/// sealed entry are each discarded without a panic. The valid chain after
/// them in the same read is accepted from its first entry, and the chain
/// rules then discard an entry with the wrong last epoch authenticator and a
/// repeated entry. A later read extends the chain. Discard logs never name
/// the group id.
// verifies: FORK-077, FORK-083
#[xmtp_common::test(unwrap_try = true)]
async fn a_reader_discards_each_entry_that_fails_fork_075() {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    bo.sync_welcomes().await?;
    let bo_group = bo.group(&alix_group.group_id)?;
    bo_group.update_consent_state(ConsentState::Allowed)?;

    alix_group
        .update_group_name("processed prefix".into())
        .await?;
    bo_group.sync().await?;

    let keys = keys_of(&bo_group);
    let seed = commit_log_seed(&root_of(&bo_group));
    let id = keys.commit_log_id();
    let group_id = bo_group.group_id.to_vec();
    let entry = |commit_sequence_id, epoch: u8| {
        let mut entry = PlaintextCommitLogEntry {
            group_id: group_id.clone(),
            commit_sequence_id,
            last_epoch_authenticator: vec![epoch; 32],
            commit_result: ProtoCommitResult::Applied as i32,
            applied_epoch_number: u64::from(epoch) + 1,
            applied_epoch_authenticator: vec![epoch + 1; 32],
            ..Default::default()
        };
        sign_entry_author(
            &keys,
            &alix.context.identity().installation_keys,
            &mut entry,
        )
        .unwrap();
        entry
    };
    let first = seal_entry(&keys, &entry(1, 1))?;
    let second = seal_entry(&keys, &entry(2, 2))?;
    // The test signer signs exactly as the commit-log signing key does.
    assert_eq!(
        signed_entry(&seed, &id, first.nonce.clone(), first.ciphertext.clone()),
        first
    );

    // Accepted, either of these would start another chain, and the valid
    // chain would then be discarded by the chain rules.
    let forged = seal_entry(&keys, &entry(1, 40))?;
    let other_root = GroupRoot::generate();
    let other_seed = commit_log_seed(&other_root);
    let mut other_group = entry(1, 30);
    other_group.group_id = vec![9; 32];
    sign_entry_author(
        &keys,
        &alix.context.identity().installation_keys,
        &mut other_group,
    )?;
    let discarded = [
        seal_entry(&GroupKeys::derive(&other_root), &entry(1, 50))?,
        signed_entry(&other_seed, &id, forged.nonce, forged.ciphertext),
        signed_entry(&seed, &id, vec![1; 12], vec![2; 32]),
        seal_entry(&keys, &other_group)?,
        signed_entry(&seed, &id, vec![], first.ciphertext.clone()),
        signed_entry(&seed, &id, vec![1; 11], first.ciphertext.clone()),
        signed_entry(&seed, &id, vec![1; 13], first.ciphertext.clone()),
        signed_entry(&seed, &id, first.nonce.clone(), vec![3; 15]),
    ];
    // Each opens under the conversation's keys; the chain rules discard both.
    let wrong_last_authenticator = seal_entry(&keys, &entry(3, 7))?;
    let repeated = seal_entry(&keys, &entry(2, 2))?;
    let topic = Topic::new_commit_log(id);
    let mut envelopes: Vec<_> = discarded
        .into_iter()
        .map(Payload::SealedCommitLogEntry)
        .chain([Payload::GroupMessage(Default::default())])
        .chain(
            [first, second, wrong_last_authenticator, repeated].map(Payload::SealedCommitLogEntry),
        )
        .enumerate()
        .map(|(i, payload)| server_envelope(&topic, i as u64 + 1, payload))
        .collect();
    let last_sequence_id = envelopes.len() as u64;

    let (client, script) = scripted::scripted(&bo).await;
    script
        .lock()
        .reads
        .insert(topic.cloned_vec(), envelopes.clone());
    let mut worker = CommitLogWorker::new(client.context.clone());
    #[cfg(not(target_arch = "wasm32"))]
    let capture = xmtp_logging::test_logging::LogCapture::new(xmtp_logging::Level::Info);
    let results = {
        #[cfg(not(target_arch = "wasm32"))]
        let _logging = tracing::dispatcher::set_default(&capture.dispatch());
        worker
            .run_test(CommitLogTestFunction::SaveRemoteCommitLog, None)
            .await?
    };
    #[cfg(not(target_arch = "wasm32"))]
    {
        let output = capture.output();
        assert!(output.contains("Discarding"), "{output}");
        assert!(
            !output.contains(&hex::encode(bo_group.group_id.as_slice())),
            "a discard log names the group id: {output}"
        );
    }
    let saved = results[0].save_remote_commit_log_results.as_ref()?;
    assert_eq!(saved[bo_group.group_id.as_slice()], 2);
    let remote = bo.context.db().get_remote_commit_log_after_cursor(
        &bo_group.group_id,
        0,
        RemoteCommitLogOrder::AscendingByRowid,
    )?;
    assert_eq!(
        remote
            .iter()
            .map(|log| (log.commit_sequence_id, log.applied_epoch_number))
            .collect::<Vec<_>>(),
        vec![(1, 2), (2, 3)]
    );
    // The discarded envelopes are read past, never evaluated again.
    assert_eq!(
        bo.context
            .db()
            .get_last_cursor(bo_group.group_id, EntityKind::CommitLogDownload)?,
        Cursor(last_sequence_id)
    );
    assert_eq!(script.lock().read, vec![vec![topic.cloned_vec()]]);

    // A later read extends the accepted chain.
    envelopes.push(server_envelope(
        &topic,
        last_sequence_id + 1,
        Payload::SealedCommitLogEntry(seal_entry(&keys, &entry(3, 3))?),
    ));
    script.lock().reads.insert(topic.cloned_vec(), envelopes);
    let results = worker
        .run_test(CommitLogTestFunction::SaveRemoteCommitLog, None)
        .await?;
    let saved = results[0].save_remote_commit_log_results.as_ref()?;
    assert_eq!(saved[bo_group.group_id.as_slice()], 1);
}

/// Restored placeholders hold no root, so a worker turn neither publishes
/// nor reads a commit log for them, even when one is allowed and has local
/// entries, and no request names a topic built from their ids.
// verifies: FORK-030
#[xmtp_common::test(unwrap_try = true)]
async fn a_restored_placeholder_is_skipped_by_publish_and_read() {
    let alix = EngineTestPeer::new().await;
    let group = alix.create_group(None, None)?;
    group.update_group_name("live".into()).await?;
    let db = alix.context.db();
    let mut placeholders = Vec::new();
    for archived_id in [vec![0x42; 16], xmtp_common::rand_vec::<32>()] {
        let row = MlsGroup::insert(
            &alix.context,
            Some(&archived_id),
            GroupMembershipState::Restored,
            ConversationType::Group,
            PolicySet::default(),
            GroupMetadataOptions::default(),
            None,
            false,
        )?;
        StoredConsentRecord::new(
            ConsentType::ConversationId,
            ConsentState::Allowed,
            hex::encode(&archived_id),
        )
        .merge_into(&db)?;
        NewLocalCommitLog {
            group_id: row.id,
            commit_sequence_id: 7,
            last_epoch_authenticator: vec![1; 32],
            commit_result: CommitResult::Success,
            error_message: None,
            applied_epoch_number: 2,
            applied_epoch_authenticator: vec![2; 32],
            sender_inbox_id: None,
            sender_installation_id: None,
            commit_type: None,
        }
        .store(&db)?;
        placeholders.push(row.id);
    }

    let (client, script) = scripted::scripted(&alix).await;
    CommitLogWorker::new(client.context.clone())._tick().await?;

    let commit_log_id = keys_of(&group).commit_log_id().to_vec();
    let script = script.lock();
    assert!(!script.read.is_empty());
    for topic in script.read.iter().flatten() {
        assert_eq!(topic[1..], commit_log_id[..], "read a placeholder's topic");
    }
    for id in script.published.iter().flatten() {
        assert_eq!(*id, commit_log_id, "published a placeholder's entry");
    }
    for id in &placeholders {
        for kind in [EntityKind::CommitLogUpload, EntityKind::CommitLogDownload] {
            assert_eq!(db.get_last_cursor(id, kind)?, Cursor(0));
        }
    }
}

/// The chain conditions, on entries that already opened.
// verifies: FORK-083
#[xmtp_common::test(unwrap_try = true)]
fn test_should_skip_remote_log_entry() {
    // Does not skip if entry meets all conditions
    let latest_saved_remote_log = RemoteCommitLog {
        rowid: 0,
        log_sequence_id: 0,
        group_id: GroupId::from([0x11u8; 16]),
        commit_sequence_id: 100,
        commit_result: CommitResult::Success,
        applied_epoch_number: 3,
        applied_epoch_authenticator: vec![0x01, 0x02, 0x03],
        author_installation_key: vec![],
    };

    let entry = PlaintextCommitLogEntry {
        group_id: vec![0x11, 0x22, 0x33],
        commit_sequence_id: 101,
        last_epoch_authenticator: vec![0x01, 0x02, 0x03],
        commit_result: 1,
        applied_epoch_number: 4,
        applied_epoch_authenticator: vec![0x01, 0x02, 0x04],
        ..Default::default()
    };
    assert!(!should_skip_remote_commit_log_entry(
        Some(&latest_saved_remote_log),
        &entry,
        true,
    ));

    // Skips if commit_sequence_id of the entry is not greater than the most recently stored entry, if one exists.
    let latest_saved_remote_log = RemoteCommitLog {
        rowid: 0,
        log_sequence_id: 0,
        group_id: GroupId::from([0x11u8; 16]),
        commit_sequence_id: 100,
        commit_result: CommitResult::Success,
        applied_epoch_number: 3,
        applied_epoch_authenticator: vec![0x01, 0x02, 0x03],
        author_installation_key: vec![],
    };
    let entry = PlaintextCommitLogEntry {
        group_id: vec![0x11, 0x22, 0x33],
        commit_sequence_id: 99,
        last_epoch_authenticator: vec![0x01, 0x02, 0x03],
        commit_result: 1,
        applied_epoch_number: 4,
        applied_epoch_authenticator: vec![0x01, 0x02, 0x04],
        ..Default::default()
    };
    assert!(should_skip_remote_commit_log_entry(
        Some(&latest_saved_remote_log),
        &entry,
        true,
    ));

    // Skips if the last_epoch_authenticator does not match the epoch_authenticator of
    // the most recently stored entry with a CommitResult of COMMIT_RESULT_APPLIED, if one exists.
    let latest_saved_remote_log = RemoteCommitLog {
        rowid: 0,
        log_sequence_id: 0,
        group_id: GroupId::from([0x11u8; 16]),
        commit_sequence_id: 100,
        commit_result: CommitResult::Success,
        applied_epoch_number: 3,
        applied_epoch_authenticator: vec![0x01, 0x02, 0x03],
        author_installation_key: vec![],
    };
    let entry = PlaintextCommitLogEntry {
        group_id: vec![0x11, 0x22, 0x33],
        commit_sequence_id: 101,
        last_epoch_authenticator: vec![0x01, 0x02, 0x05],
        commit_result: 1,
        applied_epoch_number: 4,
        applied_epoch_authenticator: vec![0x01, 0x02, 0x04],
        ..Default::default()
    };
    assert!(should_skip_remote_commit_log_entry(
        Some(&latest_saved_remote_log),
        &entry,
        true,
    ));

    // Skips if the applied_epoch_number of the entry is not exactly 1 greater than
    // the latest_applied_epoch_number of the remote validation info. (skipped from 3 to 5)
    let latest_saved_remote_log = RemoteCommitLog {
        rowid: 0,
        log_sequence_id: 0,
        group_id: GroupId::from([0x11u8; 16]),
        commit_sequence_id: 100,
        commit_result: CommitResult::Success,
        applied_epoch_number: 3,
        applied_epoch_authenticator: vec![0x01, 0x02, 0x03],
        author_installation_key: vec![],
    };
    let entry = PlaintextCommitLogEntry {
        group_id: vec![0x11, 0x22, 0x33],
        commit_sequence_id: 101,
        last_epoch_authenticator: vec![0x01, 0x02, 0x03],
        commit_result: 1,
        applied_epoch_number: 5,
        applied_epoch_authenticator: vec![0x01, 0x02, 0x04],
        ..Default::default()
    };
    assert!(should_skip_remote_commit_log_entry(
        Some(&latest_saved_remote_log),
        &entry,
        true,
    ));

    // Skips if the applied_epoch_number of the entry is not exactly 1 greater than
    // the latest_applied_epoch_number of the remote validation info. (stayed at 3)
    let latest_saved_remote_log = RemoteCommitLog {
        rowid: 0,
        log_sequence_id: 0,
        group_id: GroupId::from([0x11u8; 16]),
        commit_sequence_id: 100,
        commit_result: CommitResult::Success,
        applied_epoch_number: 3,
        applied_epoch_authenticator: vec![0x01, 0x02, 0x03],
        author_installation_key: vec![],
    };
    let entry = PlaintextCommitLogEntry {
        group_id: vec![0x11, 0x22, 0x33],
        commit_sequence_id: 101,
        last_epoch_authenticator: vec![0x01, 0x02, 0x03],
        commit_result: 1,
        applied_epoch_number: 3,
        applied_epoch_authenticator: vec![0x01, 0x02, 0x04],
        ..Default::default()
    };
    assert!(should_skip_remote_commit_log_entry(
        Some(&latest_saved_remote_log),
        &entry,
        true,
    ));

    // Skips if the applied_epoch_number of the entry is not exactly 1 greater than
    // the latest_applied_epoch_number of the remote validation info. (decreased from 3 to 2)
    let latest_saved_remote_log = RemoteCommitLog {
        rowid: 0,
        log_sequence_id: 0,
        group_id: GroupId::from([0x11u8; 16]),
        commit_sequence_id: 100,
        commit_result: CommitResult::Success,
        applied_epoch_number: 3,
        applied_epoch_authenticator: vec![0x01, 0x02, 0x03],
        author_installation_key: vec![],
    };
    let entry = PlaintextCommitLogEntry {
        group_id: vec![0x11, 0x22, 0x33],
        commit_sequence_id: 101,
        last_epoch_authenticator: vec![0x01, 0x02, 0x03],
        commit_result: 1,
        applied_epoch_number: 2,
        applied_epoch_authenticator: vec![0x01, 0x02, 0x04],
        ..Default::default()
    };
    assert!(should_skip_remote_commit_log_entry(
        Some(&latest_saved_remote_log),
        &entry,
        true,
    ));

    // Skips if entry CommitResult is not COMMIT_RESULT_APPLIED, and the epoch authenticator or epoch number does not match the most recently applied values
    let latest_saved_remote_log = RemoteCommitLog {
        rowid: 0,
        log_sequence_id: 0,
        group_id: GroupId::from([0x11u8; 16]),
        commit_sequence_id: 100,
        commit_result: CommitResult::Success,
        applied_epoch_number: 3,
        applied_epoch_authenticator: vec![0x01, 0x02, 0x03],
        author_installation_key: vec![],
    };
    let entry = PlaintextCommitLogEntry {
        group_id: vec![0x11, 0x22, 0x33],
        commit_sequence_id: 101,
        last_epoch_authenticator: vec![0x01, 0x02, 0x03],
        commit_result: 2,
        applied_epoch_number: 4,
        applied_epoch_authenticator: vec![0x01, 0x02, 0x03],
        ..Default::default()
    };
    assert!(should_skip_remote_commit_log_entry(
        Some(&latest_saved_remote_log),
        &entry,
        true,
    ));
    let entry = PlaintextCommitLogEntry {
        group_id: vec![0x11, 0x22, 0x33],
        commit_sequence_id: 101,
        last_epoch_authenticator: vec![0x01, 0x02, 0x03],
        commit_result: 2,
        applied_epoch_number: 3,
        applied_epoch_authenticator: vec![0x01, 0x02, 0x04],
        ..Default::default()
    };
    assert!(should_skip_remote_commit_log_entry(
        Some(&latest_saved_remote_log),
        &entry,
        true,
    ));
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_consecutive_entries_verification_happy_case() {
    // Disable background CommitLogWorker for deterministic testing
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;

    // Create a DM between alix and bo
    let alix_dm = alix.find_or_create_dm(bo.inbox_id(), None).await?;
    let bo_dm = bo.wait_for_welcomes().await?.first()?.to_owned();

    // Sync messages to bo
    bo_dm.sync().await?;
    // Only consented DM's are checked in the commit log
    bo_dm.update_consent_state(ConsentState::Allowed)?;
    let messages = bo_dm.find_messages(&MsgQueryArgs::default())?;
    // Should see 1 messages: group_updated
    assert_eq!(messages.len(), 1);

    // Check alix's commit log - should have exact sequence: GroupCreation, UpdateGroupMembership
    let alix_logs_before_publish = alix_dm.local_commit_log().await?;
    print_commit_log_with_types(&alix_dm).await;
    assert_commit_sequence(
        &alix_logs_before_publish,
        &[CommitType::GroupCreation, CommitType::UpdateGroupMembership],
    );

    // Setup commit log workers for both parties
    let mut alix_worker = CommitLogWorker::new(alix.context.clone());
    let mut bo_worker = CommitLogWorker::new(bo.context.clone());

    // Alix (creator) publishes the commit log entries first
    let alix_publish_results = alix_worker
        .run_test(CommitLogTestFunction::PublishCommitLogsToRemote, None)
        .await?;

    // Verify alix published entries successfully
    assert!(!alix_publish_results.is_empty());
    let alix_publish_result = alix_publish_results[0]
        .publish_commit_log_results
        .as_ref()
        .unwrap();
    assert_eq!(alix_publish_result.len(), 1);
    assert_eq!(
        alix_publish_result[0].conversation_id,
        alix_dm.group_id.to_vec()
    );
    // Only UpdateGroupMembership gets published (GroupCreation is not published to remote log)
    assert_eq!(alix_publish_result[0].num_entries_published, 1);

    // Now bo tries to download and verify the consecutive entries
    let bo_download_results = bo_worker
        .run_test(CommitLogTestFunction::SaveRemoteCommitLog, None)
        .await?;

    // Verify bo successfully downloaded and verified all entries
    assert!(!bo_download_results.is_empty());
    let bo_download_result = bo_download_results[0]
        .save_remote_commit_log_results
        .as_ref()
        .unwrap();
    assert_eq!(bo_download_result.len(), 1);
    assert!(bo_download_result.contains_key(bo_dm.group_id.as_slice()));
    assert_eq!(
        bo_download_result[bo_dm.group_id.as_slice()],
        1,
        "Bo should successfully verify the 1 entry from creator (UpdateGroupMembership)"
    );

    // Check bo's commit log after downloading and verifying entries
    let bo_logs_after_download = bo_dm.local_commit_log().await?;
    print_commit_log_with_types(&bo_dm).await;
    // Bo should only have Welcome at this point (before sending any messages)
    assert_commit_sequence(&bo_logs_after_download, &[CommitType::Welcome]);

    // Bo's first message will generate a KeyUpdate commit (key rotation)
    bo_dm
        .send_message("Message from bo".as_bytes(), SendMessageOpts::default())
        .await?;

    // Sync to alix
    alix_dm.sync().await?;
    let messages = alix_dm.find_messages(&MsgQueryArgs::default())?;
    // Should now have 2 messages: group_updated + 1 actual messages
    assert_eq!(messages.len(), 2);

    // Check bo's commit log after sending messages - should now have KeyUpdate commit
    let bo_logs_after_messages = bo_dm.local_commit_log().await?;
    print_commit_log_with_types(&bo_dm).await;
    // Bo should now have exact sequence: Welcome, KeyUpdate
    assert_commit_sequence(
        &bo_logs_after_messages,
        &[CommitType::Welcome, CommitType::KeyUpdate],
    );

    // Bo publishes his entries
    let bo_publish_results = bo_worker
        .run_test(CommitLogTestFunction::PublishCommitLogsToRemote, None)
        .await?;

    // Verify bo published entries successfully
    assert!(!bo_publish_results.is_empty());
    let bo_publish_result = bo_publish_results[0]
        .publish_commit_log_results
        .as_ref()
        .unwrap();
    assert_eq!(bo_publish_result.len(), 1);
    assert_eq!(
        bo_publish_result[0].conversation_id,
        bo_dm.group_id.to_vec()
    );
    // Bo should publish 1 commit log entry: KeyUpdate (Welcome is not published, it's only received)
    assert_eq!(bo_publish_result[0].num_entries_published, 1);

    // Alix downloads the new entries from bo
    let alix_download_results = alix_worker
        .run_test(CommitLogTestFunction::SaveRemoteCommitLog, None)
        .await?;

    // Verify alix successfully downloaded and verified bo's entries
    assert!(!alix_download_results.is_empty());
    let alix_download_result = alix_download_results[0]
        .save_remote_commit_log_results
        .as_ref()
        .unwrap();
    assert_eq!(alix_download_result.len(), 1);
    assert!(alix_download_result.contains_key(alix_dm.group_id.as_slice()));
    assert_eq!(
        alix_download_result[alix_dm.group_id.as_slice()],
        2,
        "Alix should download 2 entries total: her original UpdateGroupMembership + Bo's new KeyUpdate"
    );

    // Both parties verify consecutive entries published by each other under
    // the one commit-log id their shared root derives.
}

// verifies: FORK-087
#[xmtp_common::test(unwrap_try = true)]
async fn a_member_cannot_poison_the_commit_log() {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;

    let a = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let b = bo.wait_for_welcomes().await?.first()?.to_owned();
    a.update_group_name("shared".into()).await?;
    b.sync().await?;
    assert_eq!(
        a.epoch_authenticator().await?,
        b.epoch_authenticator().await?
    );

    // The honest prefix establishes the reader's accepted chain.
    let mut publisher = CommitLogWorker::new(alix.context.clone());
    publish_turn(&mut publisher).await;
    let mut reader = CommitLogWorker::new(alix.context.clone());
    reader
        .run_test(CommitLogTestFunction::SaveRemoteCommitLog, None)
        .await?;
    reader
        .run_test(CommitLogTestFunction::CheckForkedState, None)
        .await?;
    assert_eq!(
        alix.context
            .db()
            .get_group_commit_log_forked_status(&a.group_id)?,
        Some(false)
    );

    a.update_group_name("next".into()).await?;
    b.sync().await?;
    assert_eq!(
        a.epoch_authenticator().await?,
        b.epoch_authenticator().await?
    );
    let honest = PlaintextCommitLogEntry::from(a.local_commit_log().await?.last()?);
    let mut forged = PlaintextCommitLogEntry::from(b.local_commit_log().await?.last()?);
    assert_eq!(forged, honest);
    forged.applied_epoch_authenticator[0] ^= 1;
    assert_ne!(
        forged.applied_epoch_authenticator,
        honest.applied_epoch_authenticator
    );

    sign_entry_author(
        &keys_of(&b),
        &bo.context.identity().installation_keys,
        &mut forged,
    )?;

    // Bo uses only the shared root in his own group state and his authenticated
    // backend session. The backend stores this entry before Alix's worker runs.
    let mut stored = Vec::new();
    bo.context
        .api()
        .publish_commit_log(vec![seal_entry(&keys_of(&b), &forged)?], &mut stored)
        .await?;
    assert_eq!(stored.len(), 1);
    publish_turn(&mut publisher).await;

    let downloaded = reader
        .run_test(CommitLogTestFunction::SaveRemoteCommitLog, None)
        .await?;
    assert_eq!(
        downloaded[0].save_remote_commit_log_results.as_ref()?[a.group_id.as_slice()],
        1,
        "only the honest publisher entry is accepted"
    );
    let accepted = alix.context.db().get_remote_commit_log_after_cursor(
        &a.group_id,
        0,
        RemoteCommitLogOrder::DescendingByRowid,
    )?;
    assert_eq!(
        accepted[0].commit_sequence_id,
        honest.commit_sequence_id as i64
    );
    assert_eq!(
        accepted[0].applied_epoch_authenticator,
        honest.applied_epoch_authenticator
    );
    assert_eq!(
        accepted[0].author_installation_key,
        alix.context.installation_id().to_vec()
    );
    reader
        .run_test(CommitLogTestFunction::CheckForkedState, None)
        .await?;
    assert_eq!(
        alix.context
            .db()
            .get_group_commit_log_forked_status(&a.group_id)?,
        Some(false),
        "a non-publisher cannot forge a fork verdict"
    );
    assert_eq!(
        a.epoch_authenticator().await?,
        b.epoch_authenticator().await?
    );
}
