//! Tests for the `is_deletion_valid` function in the enrichment module.

use crate::messages::enrichment::is_deletion_valid;
use prost::Message;
use xmtp_db::group_message::{ContentType, DeliveryStatus, GroupMessageKind, StoredGroupMessage};
use xmtp_db::message_deletion::StoredMessageDeletion;
use xmtp_proto::types::GroupId;
use xmtp_proto::xmtp::mls::message_contents::{ContentTypeId, EncodedContent};

/// Create a test message with the given parameters
fn create_test_message(
    id: Vec<u8>,
    group_id: GroupId,
    sender_inbox_id: &str,
    content_type: ContentType,
    kind: GroupMessageKind,
) -> StoredGroupMessage {
    let raw_content = EncodedContent {
        r#type: Some(ContentTypeId {
            authority_id: "xmtp.org".into(),
            type_id: content_type.to_string(),
            version_major: 1,
            version_minor: 0,
        }),
        ..Default::default()
    };
    StoredGroupMessage {
        id,
        group_id,
        decrypted_message_bytes: raw_content.encode_to_vec(),
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
        expiry_unknown: false,
        expire_at_ns: None,
        sequence_id: 1,
        envelope_hash: None,
        expiry_ns: None,
        inserted_at_ns: 0,
        should_push: false,
        idempotency_key: 1000.to_string(),
    }
}

/// Create a test deletion record
fn create_test_deletion(
    id: Vec<u8>,
    group_id: GroupId,
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

// verifies: CTYPE-018
#[xmtp_common::test(unwrap_try = true)]
fn old_cached_text_requires_a_deletable_raw_identifier() {
    let group_id = GroupId::from([0x01u8; 16]);
    let mut message = create_test_message(
        vec![4, 5, 6],
        group_id,
        "sender_inbox",
        ContentType::Text,
        GroupMessageKind::Application,
    );
    let sender = create_test_deletion(vec![7], group_id, message.id.clone(), "sender_inbox", false);
    let admin = create_test_deletion(vec![8], group_id, message.id.clone(), "admin_inbox", true);
    assert!(is_deletion_valid(&sender, &message));
    assert!(is_deletion_valid(&admin, &message));

    let cases = [
        vec![0xff],
        EncodedContent::default().encode_to_vec(),
        EncodedContent {
            r#type: Some(ContentTypeId {
                authority_id: "custom.example".into(),
                type_id: "text".into(),
                version_major: 1,
                version_minor: 0,
            }),
            ..Default::default()
        }
        .encode_to_vec(),
        EncodedContent {
            r#type: Some(ContentTypeId {
                authority_id: "xmtp.org".into(),
                type_id: "text".into(),
                version_major: 99,
                version_minor: 0,
            }),
            ..Default::default()
        }
        .encode_to_vec(),
        EncodedContent {
            r#type: Some(ContentTypeId {
                authority_id: "xmtp.org".into(),
                type_id: "deleteMessage".into(),
                version_major: 1,
                version_minor: 0,
            }),
            ..Default::default()
        }
        .encode_to_vec(),
    ];
    for raw in cases {
        message.decrypted_message_bytes = raw;
        assert!(!is_deletion_valid(&sender, &message));
        assert!(!is_deletion_valid(&admin, &message));
    }
}

#[test]
fn test_invalid_deletion_unauthorized() {
    let group_id = GroupId::from([0x01u8; 16]);
    let message_id = vec![4, 5, 6];

    let message = create_test_message(
        message_id.clone(),
        group_id,
        "original_sender",
        ContentType::Text,
        GroupMessageKind::Application,
    );

    let deletion = create_test_deletion(
        vec![7, 8, 9],
        group_id,
        message_id.clone(),
        "random_user", // Not the sender
        false,         // Not a super admin
    );

    assert!(!is_deletion_valid(&deletion, &message));
}

#[test]
fn test_invalid_deletion_message_id_mismatch() {
    let group_id = GroupId::from([0x01u8; 16]);
    let message_id = vec![4, 5, 6];
    let wrong_message_id = vec![10, 11, 12];
    let sender = "sender_inbox";

    let message = create_test_message(
        message_id.clone(),
        group_id,
        sender,
        ContentType::Text,
        GroupMessageKind::Application,
    );

    // Deletion targets a different message than the one we're checking
    let deletion = create_test_deletion(
        vec![7, 8, 9],
        group_id,
        wrong_message_id, // Wrong message ID
        sender,
        false,
    );

    assert!(!is_deletion_valid(&deletion, &message));
}

// verifies: PROC-037
#[test]
fn test_invalid_deletion_cross_group_deletion_group_mismatch() {
    let group_id = GroupId::from([0x01u8; 16]);
    let other_group_id = GroupId::from([0x0au8; 16]);
    let message_id = vec![4, 5, 6];
    let sender = "sender_inbox";

    let message = create_test_message(
        message_id.clone(),
        group_id,
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

    assert!(!is_deletion_valid(&deletion, &message));
}

// verifies: CTYPE-018
#[test]
fn eligibility_depends_on_content_type_and_message_kind() {
    let group_id = GroupId::from([0x01u8; 16]);
    let message_id = vec![4, 5, 6];
    let sender = "sender_inbox";
    let deletion = create_test_deletion(vec![7, 8, 9], group_id, message_id.clone(), sender, false);

    for (content_type, kind, valid) in [
        (
            ContentType::GroupUpdated,
            GroupMessageKind::Application,
            false,
        ),
        (ContentType::Text, GroupMessageKind::MembershipChange, false),
        (
            ContentType::DeleteMessage,
            GroupMessageKind::Application,
            false,
        ),
        (
            ContentType::ReadReceipt,
            GroupMessageKind::Application,
            false,
        ),
        (ContentType::Reaction, GroupMessageKind::Application, false),
        (ContentType::Markdown, GroupMessageKind::Application, true),
        (ContentType::Reply, GroupMessageKind::Application, true),
        (ContentType::Attachment, GroupMessageKind::Application, true),
        (
            ContentType::RemoteAttachment,
            GroupMessageKind::Application,
            true,
        ),
    ] {
        let message = create_test_message(message_id.clone(), group_id, sender, content_type, kind);
        assert_eq!(
            is_deletion_valid(&deletion, &message),
            valid,
            "{content_type:?} {kind:?}"
        );
    }
}
