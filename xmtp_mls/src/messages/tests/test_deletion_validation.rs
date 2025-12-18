//! Tests for the `is_deletion_valid` function in the enrichment module.

use crate::messages::enrichment::is_deletion_valid;
use xmtp_db::group_message::{ContentType, DeliveryStatus, GroupMessageKind, StoredGroupMessage};
use xmtp_db::message_deletion::StoredMessageDeletion;

/// Create a test message with the given parameters
fn create_test_message(
    id: Vec<u8>,
    group_id: Vec<u8>,
    sender_inbox_id: &str,
    content_type: ContentType,
    kind: GroupMessageKind,
) -> StoredGroupMessage {
    StoredGroupMessage {
        id,
        group_id,
        decrypted_message_bytes: vec![],
        sent_at_ns: 1000,
        kind,
        sender_installation_id: vec![1, 2, 3],
        sender_inbox_id: sender_inbox_id.to_string(),
        delivery_status: DeliveryStatus::Published,
        content_type,
        version_major: 1,
        version_minor: 0,
        authority_id: "xmtp.org".to_string(),
        reference_id: None,
        expire_at_ns: None,
        sequence_id: 1,
        originator_id: 1,
        inserted_at_ns: 0,
    }
}

/// Create a test deletion record
fn create_test_deletion(
    id: Vec<u8>,
    group_id: Vec<u8>,
    deleted_message_id: Vec<u8>,
    deleted_by_inbox_id: &str,
    is_super_admin: bool,
) -> StoredMessageDeletion {
    StoredMessageDeletion {
        id,
        group_id,
        deleted_message_id,
        deleted_by_inbox_id: deleted_by_inbox_id.to_string(),
        is_super_admin_deletion: is_super_admin,
        deleted_at_ns: 2000,
    }
}

#[test]
fn test_valid_deletion_by_sender() {
    let group_id = vec![1, 2, 3];
    let message_id = vec![4, 5, 6];
    let sender = "sender_inbox";

    let message = create_test_message(
        message_id.clone(),
        group_id.clone(),
        sender,
        ContentType::Text,
        GroupMessageKind::Application,
    );

    let deletion = create_test_deletion(
        vec![7, 8, 9],
        group_id.clone(),
        message_id.clone(),
        sender, // Same sender deleting their own message
        false,
    );

    assert!(is_deletion_valid(&deletion, &message, &group_id));
}

#[test]
fn test_valid_deletion_by_super_admin() {
    let group_id = vec![1, 2, 3];
    let message_id = vec![4, 5, 6];

    let message = create_test_message(
        message_id.clone(),
        group_id.clone(),
        "original_sender",
        ContentType::Text,
        GroupMessageKind::Application,
    );

    let deletion = create_test_deletion(
        vec![7, 8, 9],
        group_id.clone(),
        message_id.clone(),
        "admin_inbox", // Different person, but is super admin
        true,
    );

    assert!(is_deletion_valid(&deletion, &message, &group_id));
}

#[test]
fn test_invalid_deletion_unauthorized() {
    let group_id = vec![1, 2, 3];
    let message_id = vec![4, 5, 6];

    let message = create_test_message(
        message_id.clone(),
        group_id.clone(),
        "original_sender",
        ContentType::Text,
        GroupMessageKind::Application,
    );

    let deletion = create_test_deletion(
        vec![7, 8, 9],
        group_id.clone(),
        message_id.clone(),
        "random_user", // Not the sender
        false,         // Not a super admin
    );

    assert!(!is_deletion_valid(&deletion, &message, &group_id));
}

#[test]
fn test_invalid_deletion_message_id_mismatch() {
    let group_id = vec![1, 2, 3];
    let message_id = vec![4, 5, 6];
    let wrong_message_id = vec![10, 11, 12];
    let sender = "sender_inbox";

    let message = create_test_message(
        message_id.clone(),
        group_id.clone(),
        sender,
        ContentType::Text,
        GroupMessageKind::Application,
    );

    // Deletion targets a different message than the one we're checking
    let deletion = create_test_deletion(
        vec![7, 8, 9],
        group_id.clone(),
        wrong_message_id, // Wrong message ID
        sender,
        false,
    );

    assert!(!is_deletion_valid(&deletion, &message, &group_id));
}

#[test]
fn test_invalid_deletion_cross_group_deletion_group_mismatch() {
    let group_id = vec![1, 2, 3];
    let other_group_id = vec![10, 11, 12];
    let message_id = vec![4, 5, 6];
    let sender = "sender_inbox";

    let message = create_test_message(
        message_id.clone(),
        group_id.clone(),
        sender,
        ContentType::Text,
        GroupMessageKind::Application,
    );

    // Deletion claims to be from a different group
    let deletion = create_test_deletion(
        vec![7, 8, 9],
        other_group_id, // Wrong group ID in deletion
        message_id.clone(),
        sender,
        false,
    );

    assert!(!is_deletion_valid(&deletion, &message, &group_id));
}

#[test]
fn test_invalid_deletion_message_group_mismatch() {
    let expected_group_id = vec![1, 2, 3];
    let message_group_id = vec![10, 11, 12];
    let message_id = vec![4, 5, 6];
    let sender = "sender_inbox";

    // Message is in a different group than expected
    let message = create_test_message(
        message_id.clone(),
        message_group_id, // Message is in wrong group
        sender,
        ContentType::Text,
        GroupMessageKind::Application,
    );

    let deletion = create_test_deletion(
        vec![7, 8, 9],
        expected_group_id.clone(),
        message_id.clone(),
        sender,
        false,
    );

    assert!(!is_deletion_valid(&deletion, &message, &expected_group_id));
}

#[test]
fn test_invalid_deletion_non_deletable_content_type() {
    let group_id = vec![1, 2, 3];
    let message_id = vec![4, 5, 6];
    let sender = "sender_inbox";

    // GroupUpdated is not deletable
    let message = create_test_message(
        message_id.clone(),
        group_id.clone(),
        sender,
        ContentType::GroupUpdated,
        GroupMessageKind::Application,
    );

    let deletion = create_test_deletion(
        vec![7, 8, 9],
        group_id.clone(),
        message_id.clone(),
        sender,
        false,
    );

    assert!(!is_deletion_valid(&deletion, &message, &group_id));
}

#[test]
fn test_invalid_deletion_non_deletable_message_kind() {
    let group_id = vec![1, 2, 3];
    let message_id = vec![4, 5, 6];
    let sender = "sender_inbox";

    // MembershipChange kind is not deletable
    let message = create_test_message(
        message_id.clone(),
        group_id.clone(),
        sender,
        ContentType::Text,
        GroupMessageKind::MembershipChange,
    );

    let deletion = create_test_deletion(
        vec![7, 8, 9],
        group_id.clone(),
        message_id.clone(),
        sender,
        false,
    );

    assert!(!is_deletion_valid(&deletion, &message, &group_id));
}

#[test]
fn test_invalid_deletion_delete_message_content_type() {
    let group_id = vec![1, 2, 3];
    let message_id = vec![4, 5, 6];
    let sender = "sender_inbox";

    // DeleteMessage content type should not be deletable
    let message = create_test_message(
        message_id.clone(),
        group_id.clone(),
        sender,
        ContentType::DeleteMessage,
        GroupMessageKind::Application,
    );

    let deletion = create_test_deletion(
        vec![7, 8, 9],
        group_id.clone(),
        message_id.clone(),
        sender,
        false,
    );

    assert!(!is_deletion_valid(&deletion, &message, &group_id));
}

#[test]
fn test_invalid_deletion_read_receipt_not_deletable() {
    let group_id = vec![1, 2, 3];
    let message_id = vec![4, 5, 6];
    let sender = "sender_inbox";

    // ReadReceipt is not deletable
    let message = create_test_message(
        message_id.clone(),
        group_id.clone(),
        sender,
        ContentType::ReadReceipt,
        GroupMessageKind::Application,
    );

    let deletion = create_test_deletion(
        vec![7, 8, 9],
        group_id.clone(),
        message_id.clone(),
        sender,
        false,
    );

    assert!(!is_deletion_valid(&deletion, &message, &group_id));
}

#[test]
fn test_invalid_deletion_reaction_not_deletable() {
    let group_id = vec![1, 2, 3];
    let message_id = vec![4, 5, 6];
    let sender = "sender_inbox";

    // Reaction is not deletable
    let message = create_test_message(
        message_id.clone(),
        group_id.clone(),
        sender,
        ContentType::Reaction,
        GroupMessageKind::Application,
    );

    let deletion = create_test_deletion(
        vec![7, 8, 9],
        group_id.clone(),
        message_id.clone(),
        sender,
        false,
    );

    assert!(!is_deletion_valid(&deletion, &message, &group_id));
}

#[test]
fn test_valid_deletion_markdown_content() {
    let group_id = vec![1, 2, 3];
    let message_id = vec![4, 5, 6];
    let sender = "sender_inbox";

    // Markdown is deletable
    let message = create_test_message(
        message_id.clone(),
        group_id.clone(),
        sender,
        ContentType::Markdown,
        GroupMessageKind::Application,
    );

    let deletion = create_test_deletion(
        vec![7, 8, 9],
        group_id.clone(),
        message_id.clone(),
        sender,
        false,
    );

    assert!(is_deletion_valid(&deletion, &message, &group_id));
}

#[test]
fn test_valid_deletion_reply_content() {
    let group_id = vec![1, 2, 3];
    let message_id = vec![4, 5, 6];
    let sender = "sender_inbox";

    // Reply is deletable
    let message = create_test_message(
        message_id.clone(),
        group_id.clone(),
        sender,
        ContentType::Reply,
        GroupMessageKind::Application,
    );

    let deletion = create_test_deletion(
        vec![7, 8, 9],
        group_id.clone(),
        message_id.clone(),
        sender,
        false,
    );

    assert!(is_deletion_valid(&deletion, &message, &group_id));
}

#[test]
fn test_valid_deletion_attachment_content() {
    let group_id = vec![1, 2, 3];
    let message_id = vec![4, 5, 6];
    let sender = "sender_inbox";

    // Attachment is deletable
    let message = create_test_message(
        message_id.clone(),
        group_id.clone(),
        sender,
        ContentType::Attachment,
        GroupMessageKind::Application,
    );

    let deletion = create_test_deletion(
        vec![7, 8, 9],
        group_id.clone(),
        message_id.clone(),
        sender,
        false,
    );

    assert!(is_deletion_valid(&deletion, &message, &group_id));
}

#[test]
fn test_valid_deletion_remote_attachment_content() {
    let group_id = vec![1, 2, 3];
    let message_id = vec![4, 5, 6];
    let sender = "sender_inbox";

    // RemoteAttachment is deletable
    let message = create_test_message(
        message_id.clone(),
        group_id.clone(),
        sender,
        ContentType::RemoteAttachment,
        GroupMessageKind::Application,
    );

    let deletion = create_test_deletion(
        vec![7, 8, 9],
        group_id.clone(),
        message_id.clone(),
        sender,
        false,
    );

    assert!(is_deletion_valid(&deletion, &message, &group_id));
}
