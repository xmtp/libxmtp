//! Shared message fixtures for the group_message test modules.

use super::super::*;
use crate::Store;
use xmtp_common::{Generate, rand_time, rand_vec};

pub(crate) fn generate_message(
    kind: Option<GroupMessageKind>,
    group_id: Option<&GroupId>,
    sent_at_ns: Option<i64>,
    content_type: Option<ContentType>,
    expire_at_ns: Option<i64>,
    sender_inbox_id: Option<String>,
) -> StoredGroupMessage {
    StoredGroupMessage {
        id: rand_vec::<24>(),
        group_id: group_id.copied().unwrap_or_else(GroupId::generate),
        decrypted_message_bytes: rand_vec::<24>(),
        sent_at_ns: sent_at_ns.unwrap_or(rand_time()),
        sender_installation_id: rand_vec::<24>(),
        sender_inbox_id: sender_inbox_id.unwrap_or("0x0".to_string()),
        kind: kind.unwrap_or(GroupMessageKind::Application),
        delivery_status: DeliveryStatus::Published,
        content_type: content_type.unwrap_or(ContentType::Unknown),
        version_major: 0,
        version_minor: 0,
        authority_id: "unknown".to_string(),
        reference_id: None,
        sequence_id: 0,
        envelope_hash: None,
        expiry_ns: None,

        expire_at_ns,
        inserted_at_ns: 0, // Will be set by database
        should_push: true,
        idempotency_key: String::new(),
    }
}

pub(crate) fn generate_message_with_reference<C: ConnectionExt>(
    conn: &DbConnection<C>,
    group_id: &GroupId,
    sent_at_ns: i64,
    content_type: ContentType,
    reference_id: Option<Vec<u8>>,
) -> StoredGroupMessage {
    let message = StoredGroupMessage {
        id: rand_vec::<24>(),
        group_id: *group_id,
        decrypted_message_bytes: rand_vec::<24>(),
        sent_at_ns,
        sender_installation_id: rand_vec::<24>(),
        sender_inbox_id: "0x0".to_string(),
        kind: GroupMessageKind::Application,
        delivery_status: DeliveryStatus::Published,
        content_type,
        version_major: 0,
        version_minor: 0,
        authority_id: "unknown".to_string(),
        reference_id,
        sequence_id: 0,
        envelope_hash: None,
        expiry_ns: None,

        expire_at_ns: None,
        inserted_at_ns: 0, // Will be set by database
        should_push: true,
        idempotency_key: sent_at_ns.to_string(),
    };
    message.store(conn).unwrap();
    message
}
