//! Test for self-removal and PendingRemove membership state

use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_self_removal_with_pending_state() {
    let alix = new_test_client().await;
    let bo = new_test_client().await;

    // Alix creates a group and adds Bo
    let alix_group = alix
        .conversations()
        .create_group(
            vec![bo.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();

    // Bo syncs and gets the group
    bo.conversations().sync().await.unwrap();
    let bo_group = bo.conversation(alix_group.id()).unwrap();

    // Verify Bo's membership state is Pending when first invited
    let bo_state_initial = bo_group.membership_state().unwrap();
    assert_eq!(bo_state_initial, FfiGroupMembershipState::Pending);

    // Verify Alix's membership state is Allowed (creator)
    let alix_state = alix_group.membership_state().unwrap();
    assert_eq!(alix_state, FfiGroupMembershipState::Allowed);

    // Bo leaves the group
    bo_group.leave_group().await.unwrap();

    // Verify Bo's membership state is PendingRemove after requesting to leave
    let bo_state_after_leave = bo_group.membership_state().unwrap();
    assert_eq!(bo_state_after_leave, FfiGroupMembershipState::PendingRemove);

    // Alix syncs to process the leave request
    alix_group.sync().await.unwrap();

    // Wait for admin worker to process the removal
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Bo syncs to get the final removal
    bo_group.sync().await.unwrap();

    // Verify Bo's group is no longer active
    assert!(!bo_group.is_active().unwrap());

    // Verify Alix's membership state remains Allowed
    let alix_state_final = alix_group.membership_state().unwrap();
    assert_eq!(alix_state_final, FfiGroupMembershipState::Allowed);

    // Verify only Alix remains in the group
    alix_group.sync().await.unwrap();
    let members = alix_group.list_members().await.unwrap();
    assert_eq!(members.len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_membership_state_after_readd() {
    let alix = new_test_client().await;
    let bo = new_test_client().await;

    // Alix creates a group and adds Bo
    let alix_group = alix
        .conversations()
        .create_group(
            vec![bo.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();

    // Bo syncs and gets the group
    bo.conversations().sync().await.unwrap();
    let bo_group = bo.conversation(alix_group.id()).unwrap();

    // Verify Bo's initial membership state is Pending
    let bo_state_initial = bo_group.membership_state().unwrap();
    assert_eq!(
        bo_state_initial,
        FfiGroupMembershipState::Pending,
        "Bo should be in Pending state when first invited"
    );

    // Bo leaves the group
    bo_group.leave_group().await.unwrap();

    // Verify Bo's membership state is PendingRemove after requesting to leave
    let bo_state_after_leave = bo_group.membership_state().unwrap();
    assert_eq!(
        bo_state_after_leave,
        FfiGroupMembershipState::PendingRemove,
        "Bo should be in PendingRemove state after leaving"
    );

    // Alix syncs to process the leave request
    alix_group.sync().await.unwrap();

    // Wait for admin worker to process the removal
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Bo syncs to get the final removal
    bo_group.sync().await.unwrap();

    // Verify Bo's group is no longer active
    assert!(
        !bo_group.is_active().unwrap(),
        "Bo's group should be inactive after removal"
    );

    // Alix re-adds Bo to the group
    alix_group
        .add_members(vec![bo.account_identifier.clone()])
        .await
        .unwrap();

    // Alix syncs to send the add
    alix_group.sync().await.unwrap();

    // Bo syncs to receive the welcome message for being re-added
    bo.conversations().sync().await.unwrap();

    // Bo should have the group again (same ID)
    let bo_group_after_readd = bo.conversation(alix_group.id()).unwrap();

    // CRITICAL: Verify Bo's membership state is Allowed (not PendingRemove)
    let bo_state_after_readd = bo_group_after_readd.membership_state().unwrap();
    assert_eq!(
        bo_state_after_readd,
        FfiGroupMembershipState::Allowed,
        "Bo should be in Allowed state after being re-added, not PendingRemove"
    );

    // Verify the group is active again
    assert!(
        bo_group_after_readd.is_active().unwrap(),
        "Bo's group should be active after re-add"
    );

    // Verify consent state is Unknown (user needs to accept)
    let bo_consent_after_readd = bo_group_after_readd.consent_state().unwrap();
    assert_eq!(
        bo_consent_after_readd,
        FfiConsentState::Unknown,
        "Bo's consent should be Unknown after re-add, requiring explicit acceptance"
    );

    // Verify the group shows up correctly in UX logic
    // This mimics the Android logic: isActive() && membershipState() != PENDING_REMOVE
    let is_active_and_not_pending_removal = bo_group_after_readd.is_active().unwrap()
        && bo_group_after_readd.membership_state().unwrap()
            != FfiGroupMembershipState::PendingRemove;
    assert!(
        is_active_and_not_pending_removal,
        "Group should be active and not in PendingRemove state for proper UX rendering"
    );

    // Verify both members are back in the group
    alix_group.sync().await.unwrap();
    let members_after_readd = alix_group.list_members().await.unwrap();
    assert_eq!(
        members_after_readd.len(),
        2,
        "Both Alix and Bo should be in the group"
    );
}

/// Test that leave request messages are visible and properly decoded.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_leave_request_message_is_visible() {
    let alix = new_test_client().await;
    let bo = new_test_client().await;

    // Alix creates a group and adds Bo
    let alix_group = alix
        .conversations()
        .create_group(
            vec![bo.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();

    // Bo syncs and gets the group
    bo.conversations().sync().await.unwrap();
    let bo_group = bo.conversation(alix_group.id()).unwrap();

    // Get initial message count
    let initial_messages = alix_group
        .find_messages(FfiListMessagesOptions::default())
        .await
        .unwrap();
    let initial_count = initial_messages.len();
    println!("Initial message count: {}", initial_count);

    // Bo leaves the group - this should create a LeaveRequest message
    bo_group.leave_group().await.unwrap();

    // Alix syncs to receive the leave request
    alix_group.sync().await.unwrap();

    // Get all messages after the leave request
    let messages_after_leave = alix_group
        .find_messages(FfiListMessagesOptions::default())
        .await
        .unwrap();

    println!(
        "Message count after leave request: {}",
        messages_after_leave.len()
    );

    // Print all message content types to see what we have
    for (i, msg) in messages_after_leave.iter().enumerate() {
        println!(
            "Message {}: kind={:?}, sender={}",
            i, msg.kind, msg.sender_inbox_id
        );
    }

    // The leave request message should be present
    // If it's not, it means the message is being filtered out or not stored properly
    assert!(
        messages_after_leave.len() > initial_count,
        "There should be more messages after Bo's leave request. Initial: {}, After: {}",
        initial_count,
        messages_after_leave.len()
    );

    // Wait for admin worker to process the removal
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    alix_group.sync().await.unwrap();

    // Get final messages including the GroupUpdated message from the removal
    let final_messages = alix_group
        .find_messages(FfiListMessagesOptions::default())
        .await
        .unwrap();

    println!("Final message count: {}", final_messages.len());

    // Find any messages that are from Bo (the one who left)
    let bo_messages: Vec<_> = final_messages
        .iter()
        .filter(|m| m.sender_inbox_id == bo.inner_client.inbox_id())
        .collect();

    println!("Messages from Bo: {}", bo_messages.len());
    for (i, msg) in bo_messages.iter().enumerate() {
        println!("Bo's message {}: kind={:?}", i, msg.kind);
    }

    let enriched_messages = alix_group
        .find_enriched_messages(FfiListMessagesOptions::default())
        .unwrap();

    println!("Enriched message count: {}", enriched_messages.len());

    for (i, msg) in enriched_messages.iter().enumerate() {
        println!(
            "Enriched message {}: kind={:?}, content_type={:?}",
            i,
            msg.kind(),
            msg.content_type_id()
        );
    }

    // Find the leave request message from Bo
    let bo_enriched_messages: Vec<_> = enriched_messages
        .iter()
        .filter(|m| m.sender_inbox_id() == bo.inner_client.inbox_id())
        .collect();

    println!(
        "\nEnriched messages from Bo: {}",
        bo_enriched_messages.len()
    );
    for (i, msg) in bo_enriched_messages.iter().enumerate() {
        println!(
            "Bo's enriched message {}: content_type_id={}",
            i,
            msg.content_type_id().type_id
        );
    }

    // Verify the leave request message is present and decodable
    assert!(
        !bo_enriched_messages.is_empty(),
        "Bo's leave request message should be present in enriched messages"
    );

    // Check if the leave request content type is properly identified
    let leave_request_msg = bo_enriched_messages
        .iter()
        .find(|m| m.content_type_id().type_id == "leave_request");

    if let Some(leave_msg) = leave_request_msg {
        println!("SUCCESS: Leave request message found with correct content type!");

        // Now try to access the content - LeaveRequest should be properly decoded
        let content = leave_msg.content();
        println!("Leave request content variant: {:?}", content);

        // Verify that LeaveRequest is properly decoded (not as Custom)
        match content {
            FfiDecodedMessageContent::LeaveRequest(leave_request) => {
                println!(
                    "Leave request properly decoded! authenticated_note: {:?}",
                    leave_request.authenticated_note
                );
            }
            FfiDecodedMessageContent::Custom(encoded) => {
                panic!(
                    "Leave request should NOT be decoded as Custom. Type ID: {:?}",
                    encoded.type_id
                );
            }
            _ => {
                panic!("Unexpected content variant: {:?}", content);
            }
        }
    } else {
        panic!(
            "Leave request message not found with 'leave_request' content type. \
             Available types: {:?}",
            bo_enriched_messages
                .iter()
                .map(|m| m.content_type_id().type_id.clone())
                .collect::<Vec<_>>()
        );
    }
}
