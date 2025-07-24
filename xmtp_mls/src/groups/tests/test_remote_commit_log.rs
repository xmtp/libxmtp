use crate::groups::commit_log::{CommitLogTestFunction, CommitLogWorker};
use crate::{context::XmtpContextProvider, tester};
use prost::Message;
use rand::Rng;
use xmtp_db::group::GroupQueryArgs;
use xmtp_proto::mls_v1::QueryCommitLogRequest;
use xmtp_proto::xmtp::mls::message_contents::PlaintextCommitLogEntry;

#[xmtp_common::test(unwrap_try = true)]
async fn test_commit_log_publish_and_query_apis() {
    tester!(alix);

    // There is no way to clear commit log on the local node between tests, so we'll just write to
    // a new random group_id for each test iteration in case local node state has not been cleared

    // Generate a random 20-byte group_id for this test
    let group_id: Vec<u8> = (0..20)
        .map(|_| rand::thread_rng().gen_range(0..=255))
        .collect();

    // Test publishing commit log
    let commit_log_entry = PlaintextCommitLogEntry {
        group_id: group_id.clone(),
        commit_sequence_id: 123,
        last_epoch_authenticator: vec![5, 6, 7, 8],
        commit_result: 1, // Success
        applied_epoch_number: 456,
        applied_epoch_authenticator: vec![9, 10, 11, 12],
    };

    let result = alix
        .api()
        .publish_commit_log(&[commit_log_entry.clone()])
        .await;
    assert!(result.is_ok());

    // Test querying commit log
    let query = QueryCommitLogRequest {
        group_id: group_id.clone(),
        ..Default::default()
    };

    let query_result = alix.api().query_commit_log(vec![query]).await;
    assert!(query_result.is_ok());

    // Extract the entries from the response
    let response = query_result.unwrap();
    assert_eq!(response.len(), 1);
    assert_eq!(response[0].commit_log_entries.len(), 1);

    let raw_bytes = &response[0].commit_log_entries[0].encrypted_commit_log_entry;

    // TODO(cvoell): this will require decryption once encrypted key is added
    let entry = PlaintextCommitLogEntry::decode(raw_bytes.as_slice()).unwrap();
    assert_eq!(entry.group_id, commit_log_entry.group_id);
    assert_eq!(
        entry.commit_sequence_id,
        commit_log_entry.commit_sequence_id
    );
    assert_eq!(
        entry.last_epoch_authenticator,
        commit_log_entry.last_epoch_authenticator
    );
    assert_eq!(entry.commit_result, commit_log_entry.commit_result);
    assert_eq!(
        entry.applied_epoch_number,
        commit_log_entry.applied_epoch_number
    );
    assert_eq!(
        entry.applied_epoch_authenticator,
        commit_log_entry.applied_epoch_authenticator
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_should_publish_commit_log() {
    tester!(alix);
    tester!(bo);

    let alix_group = alix.create_group(None, None).unwrap();
    alix_group
        .add_members_by_inbox_id(&[bo.inbox_id()])
        .await
        .unwrap();
    bo.sync_all_welcomes_and_groups(None).await.unwrap();

    let binding = bo.list_conversations(GroupQueryArgs::default()).unwrap();
    let bo_group = binding.first().unwrap();
    assert_eq!(bo_group.group.group_id, alix_group.group_id);

    let alix_should_publish_commit_log_groups = alix
        .find_groups(GroupQueryArgs {
            should_publish_commit_log: Some(true),
            ..Default::default()
        })
        .unwrap();

    let bo_should_publish_commit_log_groups = bo
        .find_groups(GroupQueryArgs {
            should_publish_commit_log: Some(true),
            ..Default::default()
        })
        .unwrap();

    assert_eq!(alix_should_publish_commit_log_groups.len(), 1);
    assert_eq!(bo_should_publish_commit_log_groups.len(), 0);
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_publish_commit_log_to_remote() {
    tester!(alix);
    tester!(bo);

    // Alix creates a group with Bo
    let alix_group = alix.create_group(None, None).unwrap();
    alix_group
        .add_members_by_inbox_id(&[bo.inbox_id()])
        .await
        .unwrap();
    bo.sync_all_welcomes_and_groups(None).await.unwrap();

    let binding = bo.list_conversations(GroupQueryArgs::default()).unwrap();
    let bo_group = binding.first().unwrap();
    assert_eq!(bo_group.group.group_id, alix_group.group_id);

    // Alix has two local commit log entry
    let commit_log_entries = alix
        .provider
        .db()
        .get_group_logs(&alix_group.group_id)
        .unwrap();
    assert_eq!(commit_log_entries.len(), 2);

    // Since Alix has never written to the remote commit log, the last cursor should be 0
    let published_commit_log_cursor = alix
        .provider
        .db()
        .get_last_cursor_for_id(
            &alix_group.group_id,
            xmtp_db::refresh_state::EntityKind::CommitLogUpload,
        )
        .unwrap();
    assert_eq!(published_commit_log_cursor, 0);

    // Alix runs the commit log worker, which will publish the commit log entry to the remote commit log
    let mut commit_log_worker = CommitLogWorker::new(alix.context.clone());
    let result = commit_log_worker
        .run_test(CommitLogTestFunction::PublishCommitLogsToRemote, Some(1))
        .await;
    assert!(result.is_ok());

    let published_commit_log_cursor = alix
        .provider
        .db()
        .get_last_cursor_for_id(
            &alix_group.group_id,
            xmtp_db::refresh_state::EntityKind::CommitLogUpload,
        )
        .unwrap();
    assert!(published_commit_log_cursor > 0);
    let last_commit_log_entry = commit_log_entries.last().unwrap();
    // Verify that the local cursor has now been updated to the last commit log entry's sequence id
    assert_eq!(
        last_commit_log_entry.commit_sequence_id,
        published_commit_log_cursor
    );

    // Query the remote commit log to make sure it matches the local commit log entry
    let query = QueryCommitLogRequest {
        group_id: alix_group.group_id.clone(),
        ..Default::default()
    };

    let query_result = alix.api().query_commit_log(vec![query]).await;
    assert!(query_result.is_ok());

    // Extract the entries from the response
    let response = query_result.unwrap();
    assert_eq!(response.len(), 1);
    assert_eq!(response[0].commit_log_entries.len(), 1);
    let raw_bytes = &response[0].commit_log_entries[0].encrypted_commit_log_entry;

    // TODO: this will require decryption once encrypted key is added
    let entry = PlaintextCommitLogEntry::decode(raw_bytes.as_slice()).unwrap();
    assert_eq!(
        entry.commit_sequence_id,
        last_commit_log_entry.commit_sequence_id as u64
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_download_commit_log_from_remote() {
    tester!(alix);
    tester!(bo);

    let alix_group = alix.create_group(None, None).unwrap();
    alix_group
        .add_members_by_inbox_id(&[bo.inbox_id()])
        .await
        .unwrap();

    alix_group
        .update_group_name("foo".to_string())
        .await
        .unwrap();
    alix_group
        .update_group_name("bar".to_string())
        .await
        .unwrap();

    bo.sync_all_welcomes_and_groups(None).await.unwrap();
    let binding = bo.find_groups(GroupQueryArgs::default()).unwrap();
    let bo_group = binding.first().unwrap();
    bo_group.sync().await.unwrap();
    bo_group
        .update_group_name("bo group name".to_string())
        .await
        .unwrap();
    alix_group.sync().await.unwrap();

    // Before Alix publishes commits upload commit cursor should be 0 for both groups:
    let alix_group_1_cursor = alix
        .provider
        .db()
        .get_last_cursor_for_id(
            &alix_group.group_id,
            xmtp_db::refresh_state::EntityKind::CommitLogUpload,
        )
        .unwrap();
    assert_eq!(alix_group_1_cursor, 0);

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

    // Running for bo and charlie should have no publish commit log results since they are not super admins
    let mut commit_log_worker = CommitLogWorker::new(bo.context.clone());
    let bo_test_results = commit_log_worker
        .run_test(CommitLogTestFunction::PublishCommitLogsToRemote, Some(1))
        .await
        .unwrap();
    assert_eq!(bo_test_results.len(), 1);
    assert!(bo_test_results[0]
        .publish_commit_log_results
        .as_ref()
        .unwrap()
        .is_empty());

    // Verify the number of commits published results for alix group 1
    assert_eq!(
        test_results[0].publish_commit_log_results.clone().unwrap()[0].conversation_id,
        alix_group.group_id
    );
    assert_eq!(
        test_results[0].publish_commit_log_results.clone().unwrap()[0].num_entries_published,
        4
    );

    // After Alix publishes commits upload commit cursor should be equal to publish results last rowid for both groups:
    let alix_group_1_cursor = alix
        .provider
        .db()
        .get_last_cursor_for_id(
            &alix_group.group_id,
            xmtp_db::refresh_state::EntityKind::CommitLogUpload,
        )
        .unwrap();

    let alix_group1_publish_result_upload_cursor =
        test_results[0].publish_commit_log_results.clone().unwrap()[0].last_entry_published_rowid;
    assert_eq!(
        alix_group_1_cursor,
        alix_group1_publish_result_upload_cursor
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
        alix_test_results[0]
            .save_remote_commit_log_results
            .as_ref()
            .unwrap()[0]
            .conversation_id,
        alix_group.group_id
    );
    assert_eq!(
        alix_test_results[0]
            .save_remote_commit_log_results
            .as_ref()
            .unwrap()[0]
            .num_entries_saved,
        4
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
        bo_test_results[0]
            .save_remote_commit_log_results
            .as_ref()
            .unwrap()[0]
            .conversation_id,
        bo_group.group_id
    );
    assert_eq!(
        bo_test_results[0]
            .save_remote_commit_log_results
            .as_ref()
            .unwrap()[0]
            .num_entries_saved,
        4
    );

    // Verify that cursor works as expected for saving new remote commit log entries
    alix_group
        .update_group_name("one".to_string())
        .await
        .unwrap();
    alix_group
        .update_group_name("two".to_string())
        .await
        .unwrap();
    alix_group.sync().await.unwrap();
    bo_group.sync().await.unwrap();

    let mut commit_log_worker_alix = CommitLogWorker::new(alix.context.clone());
    let alix_test_results = commit_log_worker_alix
        .run_test(CommitLogTestFunction::All, None)
        .await
        .unwrap();

    // Alix should only have saved 2 new remote commit log entries
    assert_eq!(
        alix_test_results[0]
            .publish_commit_log_results
            .as_ref()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        alix_test_results[0]
            .publish_commit_log_results
            .as_ref()
            .unwrap()[0]
            .conversation_id,
        alix_group.group_id
    );
    assert_eq!(
        alix_test_results[0]
            .publish_commit_log_results
            .as_ref()
            .unwrap()[0]
            .num_entries_published,
        2
    );
    assert_eq!(
        alix_test_results[0]
            .save_remote_commit_log_results
            .as_ref()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        alix_test_results[0]
            .save_remote_commit_log_results
            .as_ref()
            .unwrap()[0]
            .conversation_id,
        alix_group.group_id
    );
    assert_eq!(
        alix_test_results[0]
            .save_remote_commit_log_results
            .as_ref()
            .unwrap()[0]
            .num_entries_saved,
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
        .run_test(CommitLogTestFunction::All, None)
        .await
        .unwrap();

    let mut commit_log_worker_bo = CommitLogWorker::new(bo.context.clone());
    let bo_test_results = commit_log_worker_bo
        .run_test(CommitLogTestFunction::All, None)
        .await
        .unwrap();

    // Alix should have saved 2 new entries, while bo should have saved 4
    assert_eq!(
        alix_test_results[0]
            .save_remote_commit_log_results
            .as_ref()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        alix_test_results[0]
            .save_remote_commit_log_results
            .as_ref()
            .unwrap()[0]
            .conversation_id,
        alix_group.group_id
    );
    assert_eq!(
        alix_test_results[0]
            .save_remote_commit_log_results
            .as_ref()
            .unwrap()[0]
            .num_entries_saved,
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
        bo_test_results[0]
            .save_remote_commit_log_results
            .as_ref()
            .unwrap()[0]
            .conversation_id,
        bo_group.group_id
    );
    assert_eq!(
        bo_test_results[0]
            .save_remote_commit_log_results
            .as_ref()
            .unwrap()[0]
            .num_entries_saved,
        4
    );
}
