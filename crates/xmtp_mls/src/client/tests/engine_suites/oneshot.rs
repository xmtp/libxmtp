//! The leaves of `groups/oneshot.rs` that need the client.

use crate::groups::oneshot::Oneshot;
use crate::groups::{MlsGroup, PreconfiguredPolicies};
use crate::tester;
use futures::stream::StreamExt;
use xmtp_db::group::ConversationType;
use xmtp_mls_group_state::group::GroupMetadataOptions;
use xmtp_proto::xmtp::mls::message_contents::{
    OneshotMessage, ReaddRequest, oneshot_message::MessageType,
};

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
