use crate::{context::XmtpContextProvider, tester};
use prost::Message;
use rand::Rng;
use xmtp_db::group::GroupQueryArgs;
use xmtp_proto::mls_v1::QueryCommitLogRequest;
use xmtp_proto::xmtp::mls::message_contents::PlaintextCommitLogEntry;

#[xmtp_common::test(unwrap_try = true)]
async fn test_commit_log_publish_and_query() {
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
        .publish_commit_log(vec![commit_log_entry.clone()])
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
