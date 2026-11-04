//! Stored message rows that message-list tests seed.

use crate::groups::QueryableContentFields;
use crate::messages::decoded_message::DecodedMessage;
use xmtp_common::time::now_ns;
use xmtp_db::Store;
use xmtp_db::group_message::{
    ContentType as DbContentType, DeliveryStatus, GroupMessageKind, StoredGroupMessage,
};
use xmtp_proto::types::GroupId;
use xmtp_proto::xmtp::mls::message_contents::{ContentTypeId, EncodedContent};

/// An application message row of `group_id` holding `encoded_content`.
pub fn create_test_message(
    group_id: &GroupId,
    message_id: Vec<u8>,
    encoded_content: EncodedContent,
    sent_at_ns: i64,
) -> StoredGroupMessage {
    let content_bytes = xmtp_content_types::encoded_content_to_bytes(encoded_content.clone());
    let queryable_fields = QueryableContentFields::try_from(encoded_content).unwrap_or_default();

    StoredGroupMessage {
        id: message_id,
        group_id: *group_id,
        decrypted_message_bytes: content_bytes,
        sent_at_ns,
        kind: GroupMessageKind::Application,
        sender_installation_id: vec![1, 2, 3],
        sender_inbox_id: "sender".into(),
        delivery_status: DeliveryStatus::Published,
        content_type: queryable_fields.content_type,
        version_major: queryable_fields.version_major,
        version_minor: queryable_fields.version_minor,
        authority_id: queryable_fields.authority_id,
        reference_id: queryable_fields.reference_id,
        sequence_id: 0,
        envelope_hash: None,
        expiry_ns: None,
        expiry_unknown: false,
        deleted_by_inbox_id: None,
        expire_at_ns: None,
        inserted_at_ns: 0,
        should_push: true,
        idempotency_key: sent_at_ns.to_string(),
    }
}

/// An application message row of `group_id` holding the raw `content` bytes,
/// labeled with `content_type_id`, or as text when it is absent.
pub fn create_test_message_raw(
    group_id: &GroupId,
    message_id: Vec<u8>,
    content: Vec<u8>,
    sent_at_ns: i64,
    content_type_id: Option<ContentTypeId>,
    reference_id: Option<Vec<u8>>,
) -> StoredGroupMessage {
    let queryable_fields = content_type_id
        .as_ref()
        .map(|ct| {
            (
                DbContentType::from_identifier(&ct.authority_id, &ct.type_id, ct.version_major),
                ct.version_major as i32,
                ct.version_minor as i32,
                ct.authority_id.clone(),
            )
        })
        .unwrap_or((DbContentType::Text, 1, 0, "xmtp.org".to_string()));

    StoredGroupMessage {
        id: message_id,
        group_id: *group_id,
        decrypted_message_bytes: content,
        sent_at_ns,
        kind: GroupMessageKind::Application,
        sender_installation_id: vec![1, 2, 3],
        sender_inbox_id: "sender".into(),
        delivery_status: DeliveryStatus::Published,
        content_type: queryable_fields.0,
        version_major: queryable_fields.1,
        version_minor: queryable_fields.2,
        authority_id: queryable_fields.3,
        reference_id,
        sequence_id: 0,
        envelope_hash: None,
        expiry_ns: None,
        expiry_unknown: false,
        deleted_by_inbox_id: None,
        expire_at_ns: None,
        inserted_at_ns: 0,
        should_push: true,
        idempotency_key: sent_at_ns.to_string(),
    }
}

/// Store message `n` of `group_id` with `content`: its id is `n` repeated and
/// it is sent `n` microseconds after now.
pub fn create_and_store_message<S>(conn: &S, group_id: &GroupId, n: u8, content: EncodedContent)
where
    StoredGroupMessage: Store<S>,
{
    let sent_at_ns = now_ns() + i64::from(n) * 1000;
    create_test_message(group_id, vec![n; 32], content, sent_at_ns)
        .store(conn)
        .unwrap();
}

/// The hex reference to message `n`.
pub fn ref_to(n: u8) -> String {
    hex::encode([n; 32])
}

/// The first byte of each message id, in order.
pub fn ids(messages: &[DecodedMessage]) -> Vec<u8> {
    messages
        .iter()
        .map(|message| message.metadata.id[0])
        .collect()
}
