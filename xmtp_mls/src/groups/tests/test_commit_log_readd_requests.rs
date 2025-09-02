use crate::{groups::commit_log::CommitLogWorker, tester};
use xmtp_db::{Store, group::StoredGroupForReaddRequest, readd_status::ReaddStatus};

#[xmtp_common::test]
async fn test_is_awaiting_readd_no_status() {
    tester!(client);
    let group = client
        .create_group_with_inbox_ids(&Vec::<String>::new(), None, None)
        .await
        .unwrap();

    let worker = CommitLogWorker::new(client.context.clone());

    // Should return false when no readd status exists
    let result = worker
        .test_is_awaiting_readd(group.group_id.as_slice())
        .unwrap();
    assert!(!result);
}

#[xmtp_common::test]
async fn test_is_awaiting_readd_no_request() {
    tester!(client);
    let group = client
        .create_group_with_inbox_ids(&Vec::<String>::new(), None, None)
        .await
        .unwrap();

    let conn = client.context.db();

    // Create a readd status without a requested_at_sequence_id
    ReaddStatus {
        group_id: group.group_id.clone(),
        inbox_id: client.inbox_id().to_string(),
        installation_id: client.context.installation_id().to_vec(),
        requested_at_sequence_id: None,
        responded_at_sequence_id: Some(5),
    }
    .store(&conn)
    .unwrap();

    let worker = CommitLogWorker::new(client.context.clone());

    // Should return false when no request has been made
    let result = worker
        .test_is_awaiting_readd(group.group_id.as_slice())
        .unwrap();
    assert!(!result);
}

#[xmtp_common::test]
async fn test_is_awaiting_readd_request_pending() {
    tester!(client);
    let group = client
        .create_group_with_inbox_ids(&Vec::<String>::new(), None, None)
        .await
        .unwrap();

    let conn = client.context.db();

    // Create a readd status with requested_at > responded_at
    ReaddStatus {
        group_id: group.group_id.clone(),
        inbox_id: client.inbox_id().to_string(),
        installation_id: client.context.installation_id().to_vec(),
        requested_at_sequence_id: Some(10),
        responded_at_sequence_id: Some(5),
    }
    .store(&conn)
    .unwrap();

    let worker = CommitLogWorker::new(client.context.clone());

    // Should return true when request is pending
    let result = worker
        .test_is_awaiting_readd(group.group_id.as_slice())
        .unwrap();
    assert!(result);
}

#[xmtp_common::test]
async fn test_is_awaiting_readd_request_fulfilled() {
    tester!(client);
    let group = client
        .create_group_with_inbox_ids(&Vec::<String>::new(), None, None)
        .await
        .unwrap();

    let conn = client.context.db();

    // Create a readd status with requested_at <= responded_at
    ReaddStatus {
        group_id: group.group_id.clone(),
        inbox_id: client.inbox_id().to_string(),
        installation_id: client.context.installation_id().to_vec(),
        requested_at_sequence_id: Some(5),
        responded_at_sequence_id: Some(10),
    }
    .store(&conn)
    .unwrap();

    let worker = CommitLogWorker::new(client.context.clone());

    // Should return false when request has been fulfilled
    let result = worker
        .test_is_awaiting_readd(group.group_id.as_slice())
        .unwrap();
    assert!(!result);
}

#[xmtp_common::test]
async fn test_is_awaiting_readd_equal_sequence_ids() {
    tester!(client);
    let group = client
        .create_group_with_inbox_ids(&Vec::<String>::new(), None, None)
        .await
        .unwrap();

    let conn = client.context.db();

    // Create a readd status with requested_at == responded_at
    ReaddStatus {
        group_id: group.group_id.clone(),
        inbox_id: client.inbox_id().to_string(),
        installation_id: client.context.installation_id().to_vec(),
        requested_at_sequence_id: Some(10),
        responded_at_sequence_id: Some(10),
    }
    .store(&conn)
    .unwrap();

    let worker = CommitLogWorker::new(client.context.clone());

    // Should return true when sequence IDs are equal.
    // The response to a readd request will always add a commit, which increases the sequence ID.
    // It is possible that a readd request is subsequently issued at the same sequence ID.
    let result = worker
        .test_is_awaiting_readd(group.group_id.as_slice())
        .unwrap();
    assert!(result);
}

#[xmtp_common::test]
async fn test_is_awaiting_readd_no_responded_at() {
    tester!(client);
    let group = client
        .create_group_with_inbox_ids(&Vec::<String>::new(), None, None)
        .await
        .unwrap();

    let conn = client.context.db();

    // Create a readd status with requested_at but no responded_at (defaults to 0)
    ReaddStatus {
        group_id: group.group_id.clone(),
        inbox_id: client.inbox_id().to_string(),
        installation_id: client.context.installation_id().to_vec(),
        requested_at_sequence_id: Some(5),
        responded_at_sequence_id: None,
    }
    .store(&conn)
    .unwrap();

    let worker = CommitLogWorker::new(client.context.clone());

    // Should return true when requested_at > 0 (default responded_at)
    let result = worker
        .test_is_awaiting_readd(group.group_id.as_slice())
        .unwrap();
    assert!(result);
}

#[xmtp_common::test]
async fn test_request_readd_already_pending() {
    tester!(client);
    let group = client
        .create_group_with_inbox_ids(&Vec::<String>::new(), None, None)
        .await
        .unwrap();

    let conn = client.context.db();

    // Create a readd status with pending request
    ReaddStatus {
        group_id: group.group_id.clone(),
        inbox_id: client.inbox_id().to_string(),
        installation_id: client.context.installation_id().to_vec(),
        requested_at_sequence_id: Some(10),
        responded_at_sequence_id: Some(5),
    }
    .store(&conn)
    .unwrap();

    let mut worker = CommitLogWorker::new(client.context.clone());

    let group_info = StoredGroupForReaddRequest {
        group_id: group.group_id.clone(),
        latest_commit_sequence_id: Some(15),
    };

    // Should return Ok without doing anything when request is already pending
    let result = worker.test_request_readd(group_info).await;
    assert!(result.is_ok());
}

#[xmtp_common::test]
async fn test_send_readd_requests_no_groups() {
    tester!(client);
    let mut worker = CommitLogWorker::new(client.context.clone());

    // Should return Ok when no groups need readd requests
    let result = worker.test_send_readd_requests().await;
    assert!(result.is_ok());
}
