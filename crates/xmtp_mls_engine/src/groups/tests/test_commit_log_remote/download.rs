//! Remote commit log tests: download.

use super::*;

#[xmtp_common::test(unwrap_try = true)]
async fn test_download_commit_log_from_remote() {
    // Disable background CommitLogWorker for deterministic testing
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;

    // Alix creates a group with Bo (1 commit)
    let alix_group = alix.create_group(None, None).unwrap();
    alix_group.add_members(&[bo.inbox_id()]).await.unwrap();

    // Alix updates the group name (2 commits)
    alix_group
        .update_group_name("foo".to_string())
        .await
        .unwrap();

    // Alix updates the group name again (3 commits)
    alix_group
        .update_group_name("bar".to_string())
        .await
        .unwrap();

    bo.sync_all_welcomes_and_groups(None).await.unwrap();
    let binding = bo.find_groups(GroupQueryArgs::default()).unwrap();
    let bo_group = binding.first().unwrap();
    bo_group.sync().await.unwrap();

    // Bo sends a message which updates the group to be consent state allowed
    // and queues a key update intent (4 commits)
    bo_group
        .send_message(b"foo", SendMessageOpts::default())
        .await
        .unwrap();

    // Bo updates the group name (5 commits)
    bo_group
        .update_group_name("bo group name".to_string())
        .await
        .unwrap();
    alix_group.sync().await.unwrap();

    // Before Alix publishes commits upload commit cursor should be 0 for both groups:
    let alix_group_1_cursor = alix
        .context
        .db()
        .get_last_cursor(
            alix_group.group_id,
            xmtp_db::refresh_state::EntityKind::CommitLogUpload,
        )
        .unwrap();
    assert_eq!(alix_group_1_cursor, Cursor(0));

    // Verify that publish works as expected
    let mut commit_log_worker = CommitLogWorker::new(alix.context.clone());
    let test_results = commit_log_worker
        .run_test(CommitLogTestFunction::PublishCommitLogsToRemote, Some(1))
        .await
        .unwrap();
    // We only ran the worker one time
    assert_eq!(test_results.len(), 1);
    // We published commit log entries for one group
    assert_eq!(
        test_results[0]
            .publish_commit_log_results
            .as_ref()
            .unwrap()
            .len(),
        1
    );
    // We have saved zero remote commit log entries so far
    assert!(test_results[0].save_remote_commit_log_results.is_none());

    // Running for bo  should have no publish commit log results since they are not super admins
    let mut commit_log_worker = CommitLogWorker::new(bo.context.clone());
    let bo_test_results = commit_log_worker
        .run_test(CommitLogTestFunction::PublishCommitLogsToRemote, Some(1))
        .await
        .unwrap();
    assert_eq!(bo_test_results.len(), 1);
    assert!(
        bo_test_results[0]
            .publish_commit_log_results
            .as_ref()
            .unwrap()
            .is_empty()
    );

    // Verify the number of commits published results for alix group 1
    assert_eq!(
        test_results[0].publish_commit_log_results.clone().unwrap()[0]
            .conversation_id
            .as_slice(),
        alix_group.group_id.as_slice()
    );
    // We should have published 4 commits
    assert_eq!(
        test_results[0].publish_commit_log_results.clone().unwrap()[0].num_entries_published,
        5
    );

    // After Alix publishes commits upload commit cursor should be equal to publish results last rowid:
    let alix_group_1_cursor = alix
        .context
        .db()
        .get_last_cursor(
            alix_group.group_id,
            xmtp_db::refresh_state::EntityKind::CommitLogUpload,
        )
        .unwrap();

    let alix_group1_publish_result_upload_cursor =
        test_results[0].publish_commit_log_results.clone().unwrap()[0].last_entry_published_rowid;
    assert_eq!(
        alix_group_1_cursor,
        Cursor(alix_group1_publish_result_upload_cursor as u64)
    );

    // Verify that when we save remote commit log entries for alix and bo, that we get the same results
    let mut commit_log_worker_alix = CommitLogWorker::new(alix.context.clone());
    let alix_test_results = commit_log_worker_alix
        .run_test(CommitLogTestFunction::SaveRemoteCommitLog, None)
        .await
        .unwrap();
    let mut commit_log_worker_bo = CommitLogWorker::new(bo.context.clone());
    let bo_test_results = commit_log_worker_bo
        .run_test(CommitLogTestFunction::SaveRemoteCommitLog, None)
        .await
        .unwrap();
    assert_eq!(alix_test_results.len(), 1);
    assert_eq!(
        alix_test_results[0]
            .save_remote_commit_log_results
            .as_ref()
            .unwrap()
            .len(),
        1
    );

    assert_eq!(
        *alix_test_results[0]
            .save_remote_commit_log_results
            .as_ref()
            .unwrap()
            .get(alix_group.group_id.as_slice())
            .unwrap(),
        5
    );

    assert_eq!(bo_test_results.len(), 1);
    assert_eq!(
        bo_test_results[0]
            .save_remote_commit_log_results
            .as_ref()
            .unwrap()
            .len(),
        1
    );

    assert_eq!(
        *bo_test_results[0]
            .save_remote_commit_log_results
            .as_ref()
            .unwrap()
            .get(alix_group.group_id.as_slice())
            .unwrap(),
        5
    );
    // Verify that cursor works as expected for saving new remote commit log entries
    // Alix updates the group name (1 new commit (6 total))
    alix_group
        .update_group_name("one".to_string())
        .await
        .unwrap();

    // Alix updates the group name again (2 new commits (8 total))
    alix_group
        .update_group_name("two".to_string())
        .await
        .unwrap();
    alix_group.sync().await.unwrap();
    bo_group.sync().await.unwrap();

    let mut commit_log_worker_alix = CommitLogWorker::new(alix.context.clone());
    // Alix publishes commits and saves remote commit log entries
    // We run the worker twice since publish happens after save
    let alix_test_results = commit_log_worker_alix
        .run_test(CommitLogTestFunction::All, Some(2))
        .await
        .unwrap();

    // Alix should published for one conversation
    assert_eq!(
        alix_test_results[0]
            .publish_commit_log_results
            .as_ref()
            .unwrap()
            .len(),
        1
    );
    // The publish matches the conversation id
    assert_eq!(
        alix_test_results[0]
            .publish_commit_log_results
            .as_ref()
            .unwrap()[0]
            .conversation_id
            .as_slice(),
        alix_group.group_id.as_slice()
    );
    // We published 2 new commits
    assert_eq!(
        alix_test_results[0]
            .publish_commit_log_results
            .as_ref()
            .unwrap()[0]
            .num_entries_published,
        2
    );
    // We saved results for one conversation on the second worker run
    assert_eq!(
        alix_test_results[1]
            .save_remote_commit_log_results
            .as_ref()
            .unwrap()
            .len(),
        1
    );

    // We should have saved 2 new entries...
    assert_eq!(
        *alix_test_results[1]
            .save_remote_commit_log_results
            .as_ref()
            .unwrap()
            .get(alix_group.group_id.as_slice())
            .unwrap(),
        2
    );

    alix_group
        .update_group_name("three".to_string())
        .await
        .unwrap();
    alix_group
        .update_group_name("four".to_string())
        .await
        .unwrap();
    alix_group.sync().await.unwrap();
    bo_group.sync().await.unwrap();

    let mut commit_log_worker_alix = CommitLogWorker::new(alix.context.clone());
    let alix_test_results = commit_log_worker_alix
        .run_test(CommitLogTestFunction::All, Some(2))
        .await
        .unwrap();

    let mut commit_log_worker_bo = CommitLogWorker::new(bo.context.clone());
    let bo_test_results = commit_log_worker_bo
        .run_test(CommitLogTestFunction::All, None)
        .await
        .unwrap();

    // Alix should have saved 2 new entries, while bo should have saved 4
    assert_eq!(
        alix_test_results[1]
            .save_remote_commit_log_results
            .as_ref()
            .unwrap()
            .len(),
        1
    );

    assert_eq!(
        *alix_test_results[1]
            .save_remote_commit_log_results
            .as_ref()
            .unwrap()
            .get(bo_group.group_id.as_slice())
            .unwrap(),
        2
    );

    assert_eq!(
        bo_test_results[0]
            .save_remote_commit_log_results
            .as_ref()
            .unwrap()
            .len(),
        1
    );

    assert_eq!(
        *bo_test_results[0]
            .save_remote_commit_log_results
            .as_ref()
            .unwrap()
            .get(bo_group.group_id.as_slice())
            .unwrap(),
        4
    );
}

// verifies: FORK-087
#[xmtp_common::test(unwrap_try = true)]
async fn an_entry_ahead_of_the_reader_is_held() {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let caro = EngineTestPeer::new().await;
    let a = alix
        .create_group_with_members(&[bo.inbox_id(), caro.inbox_id()], None, None)
        .await?;
    bo.sync_welcomes().await?;
    caro.sync_welcomes().await?;
    let b = bo.group(&a.group_id)?;
    let c = caro.group(&a.group_id)?;
    b.update_consent_state(ConsentState::Allowed)?;
    c.update_consent_state(ConsentState::Allowed)?;
    a.update_admin_list(
        crate::groups::UpdateAdminListType::AddSuper,
        bo.inbox_id().to_string(),
    )
    .await?;
    b.sync().await?;
    publish_turn(&mut CommitLogWorker::new(bo.context.clone())).await;
    let mut reader = CommitLogWorker::new(caro.context.clone());
    reader
        .run_test(CommitLogTestFunction::SaveRemoteCommitLog, None)
        .await?;
    assert!(
        caro.context
            .db()
            .get_latest_remote_log_for_group(&a.group_id)?
            .is_none()
    );
    assert_eq!(
        caro.context
            .db()
            .get_last_cursor(a.group_id, EntityKind::CommitLogDownload)?
            .0,
        0
    );
    c.sync().await?;
    reader
        .run_test(CommitLogTestFunction::SaveRemoteCommitLog, None)
        .await?;
    assert_eq!(
        caro.context
            .db()
            .get_latest_remote_log_for_group(&a.group_id)??
            .author_installation_key,
        bo.context.installation_id().to_vec()
    );
}

// verifies: FORK-087
#[xmtp_common::test(unwrap_try = true)]
async fn an_inactive_reader_does_not_hold() {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let a = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    bo.sync_welcomes().await?;
    let b = bo.group(&a.group_id)?;
    b.update_consent_state(ConsentState::Allowed)?;
    a.remove_members(&[bo.inbox_id()]).await?;
    b.sync_with_conn().await?;
    assert!(!b.is_active()?);
    a.update_group_name("after removal".into()).await?;
    publish_turn(&mut CommitLogWorker::new(alix.context.clone())).await;
    let topic = Topic::new_commit_log(keys_of(&a).commit_log_id());
    let entries = alix
        .context
        .api()
        .query_commit_log([(topic, Cursor(0))].into())
        .await?;
    let last = entries.last()?.cursor;
    CommitLogWorker::new(bo.context.clone())
        .run_test(CommitLogTestFunction::SaveRemoteCommitLog, None)
        .await?;
    assert_eq!(
        bo.context
            .db()
            .get_last_cursor(a.group_id, EntityKind::CommitLogDownload)?,
        last
    );
}

// verifies: FORK-030, FORK-087
#[xmtp_common::test(unwrap_try = true)]
async fn a_held_entry_stops_later_pages_of_its_topic() {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let a = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    bo.sync_welcomes().await?;
    let b = bo.group(&a.group_id)?;
    b.update_consent_state(ConsentState::Allowed)?;
    a.update_group_name("reader processed prefix".into())
        .await?;
    b.sync().await?;
    a.update_group_name("reader has not processed this commit".into())
        .await?;
    let future = a.local_commit_log().await?.last()?.commit_sequence_id as u64;
    let keys = keys_of(&a);
    let topic = Topic::new_commit_log(keys.commit_log_id());
    let entry = |sequence, epoch: u64, author: &xmtp_cryptography::XmtpInstallationCredential| {
        let mut entry = PlaintextCommitLogEntry {
            group_id: a.group_id.to_vec(),
            commit_sequence_id: sequence,
            commit_result: ProtoCommitResult::Applied as i32,
            last_epoch_authenticator: vec![epoch as u8; 32],
            applied_epoch_number: epoch + 1,
            applied_epoch_authenticator: vec![epoch as u8 + 1; 32],
            ..Default::default()
        };
        sign_entry_author(&keys, author, &mut entry).unwrap();
        seal_entry(&keys, &entry).unwrap()
    };
    let envelope_base = future + 100;
    let entries = [
        entry(1, 1, &alix.context.identity().installation_keys),
        entry(future, 2, &bo.context.identity().installation_keys),
        entry(2, 2, &alix.context.identity().installation_keys),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, entry)| {
        server_envelope(
            &topic,
            envelope_base + index as u64,
            Payload::SealedCommitLogEntry(entry),
        )
    })
    .collect();
    let (client, script) = scripted::scripted_with_page_size(&bo, 2).await;
    script.lock().reads.insert(topic.cloned_vec(), entries);
    let mut reader = CommitLogWorker::new(client.context.clone());
    reader
        .run_test(CommitLogTestFunction::SaveRemoteCommitLog, None)
        .await?;
    assert_eq!(
        script.lock().read.len(),
        1,
        "held topics must not request page two"
    );
    assert_eq!(
        bo.context
            .db()
            .get_last_cursor(b.group_id, EntityKind::CommitLogDownload)?
            .0,
        envelope_base
    );
    assert_eq!(
        bo.context
            .db()
            .get_latest_remote_log_for_group(&b.group_id)??
            .commit_sequence_id,
        1
    );
    b.sync().await?;
    reader
        .run_test(CommitLogTestFunction::SaveRemoteCommitLog, None)
        .await?;
    let accepted = bo.context.db().get_remote_commit_log_after_cursor(
        &b.group_id,
        0,
        RemoteCommitLogOrder::AscendingByRowid,
    )?;
    assert_eq!(
        accepted
            .iter()
            .map(|entry| entry.commit_sequence_id)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(
        bo.context
            .db()
            .get_last_cursor(b.group_id, EntityKind::CommitLogDownload)?
            .0,
        envelope_base + 2
    );
}

/// A download stores each page and moves the cursor past it before it
/// requests the next one. A reader stopped after three pages keeps their
/// entries, and the next download starts with the fourth page and checks it
/// against the entries already stored.
// verifies: FORK-030, FORK-083
#[xmtp_common::test(unwrap_try = true)]
async fn commit_log_download_commits_each_page() {
    const ENTRIES: u64 = 16;
    const PAGE_ROWS: u32 = 2;
    const STORED_PAGES: usize = 3;
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let a = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    bo.sync_welcomes().await?;
    let b = bo.group(&a.group_id)?;
    b.update_consent_state(ConsentState::Allowed)?;
    // An active reader holds an entry for a commit it has not processed, so
    // it first processes more envelopes than there are entries.
    for _ in 0..ENTRIES {
        a.send_message_optimistic(b"padding", SendMessageOpts::default())?;
    }
    a.publish_messages().await?;
    b.sync().await?;
    let processed = bo
        .context
        .db()
        .latest_cursor_for_id(b.group_id, &[EntityKind::ApplicationMessage])?
        .0;
    assert!(processed >= ENTRIES);

    let keys = keys_of(&a);
    let topic = Topic::new_commit_log(keys.commit_log_id());
    let envelope_base = processed + 100;
    // The first envelope after the stored pages replays the last stored commit
    // with another result, which the chain rules discard.
    let stored = STORED_PAGES as u64 * u64::from(PAGE_ROWS);
    let mut commits: Vec<_> = (1..=ENTRIES)
        .map(|sequence| (sequence, sequence as u8 + 1))
        .collect();
    commits.insert(stored as usize, (stored, 0xee));
    let envelopes = commits.len() as u64;
    let entries = commits
        .into_iter()
        .zip(1..)
        .map(|((sequence, applied), position)| {
            let mut entry = PlaintextCommitLogEntry {
                group_id: a.group_id.to_vec(),
                commit_sequence_id: sequence,
                commit_result: ProtoCommitResult::Applied as i32,
                last_epoch_authenticator: vec![sequence as u8; 32],
                applied_epoch_number: sequence + 1,
                applied_epoch_authenticator: vec![applied; 32],
                ..Default::default()
            };
            sign_entry_author(
                &keys,
                &alix.context.identity().installation_keys,
                &mut entry,
            )
            .unwrap();
            server_envelope(
                &topic,
                envelope_base + position,
                Payload::SealedCommitLogEntry(seal_entry(&keys, &entry).unwrap()),
            )
        })
        .collect();
    let (client, script) = scripted::scripted_with_page_size(&bo, PAGE_ROWS).await;
    {
        let mut script = script.lock();
        script.reads.insert(topic.cloned_vec(), entries);
        script.stop_reads_after = Some(STORED_PAGES);
    }
    let accepted = || {
        bo.context
            .db()
            .get_remote_commit_log_after_cursor(
                &b.group_id,
                0,
                RemoteCommitLogOrder::AscendingByRowid,
            )
            .unwrap()
            .iter()
            .map(|entry| entry.commit_sequence_id as u64)
            .collect::<Vec<_>>()
    };
    let download_cursor = || {
        bo.context
            .db()
            .get_last_cursor(b.group_id, EntityKind::CommitLogDownload)
            .unwrap()
            .0
    };

    assert!(
        CommitLogWorker::new(client.context.clone())
            .run_test(CommitLogTestFunction::SaveRemoteCommitLog, None)
            .await
            .is_err()
    );
    assert_eq!(accepted(), (1..=stored).collect::<Vec<_>>());
    assert_eq!(download_cursor(), envelope_base + stored);

    {
        let mut script = script.lock();
        script.stop_reads_after = None;
        script.read.clear();
    }
    CommitLogWorker::new(client.context.clone())
        .run_test(CommitLogTestFunction::SaveRemoteCommitLog, None)
        .await?;
    assert_eq!(
        script.lock().read.len() as u64,
        (envelopes - stored).div_ceil(u64::from(PAGE_ROWS)),
        "the resumed download starts after the stored pages"
    );
    assert_eq!(accepted(), (1..=ENTRIES).collect::<Vec<_>>());
    assert_eq!(download_cursor(), envelope_base + envelopes);
}
