use std::collections::HashSet;

use xmtp_common::Generate;
use xmtp_db::group_message::StoredGroupMessage;
use xmtp_proto::types::Cursor;

use crate::groups::{
    GroupMessageProcessingError,
    summary::{MessageIdentifier, ProcessSummary, SyncSummary},
};

use super::*;
use rstest::*;

#[fixture]
pub fn context() -> NewMockContext {
    crate::context::test_support::mock_context()
}

pub use xmtp_id::utils::generate_inbox_id_credential;

pub fn generate_messages_with_ids(ids: &[u64]) -> Vec<xmtp_proto::types::GroupMessage> {
    ids.iter()
        .map(|id| generate_message(*id, &xmtp_proto::types::GroupId::generate()))
        .collect()
}

pub fn generate_message(
    cursor: u64,
    group_id: &xmtp_proto::types::GroupId,
) -> xmtp_proto::types::GroupMessage {
    let mut msg = xmtp_proto::types::GroupMessage::generate();
    msg.cursor.0 = cursor;

    msg.group_id = *group_id;
    msg
}

pub fn generate_successful_summary(messages: &[xmtp_proto::types::GroupMessage]) -> SyncSummary {
    let mut summary = SyncSummary::default();
    summary.add_process(ProcessSummary {
        total_messages: HashSet::from_iter(messages.iter().map(|m| m.cursor)),
        new_messages: messages.iter().map(Into::into).collect(),
        errored: Vec::new(),
        app_data_changes: Vec::new(),
    });
    summary
}

pub fn generate_errored_summary(error_cursors: &[u64], successful_cursors: &[u64]) -> SyncSummary {
    let mut summary = SyncSummary::default();
    summary.add_process(ProcessSummary {
        total_messages: HashSet::from_iter(
            error_cursors
                .iter()
                .copied()
                .chain(successful_cursors.iter().copied())
                .map(Cursor),
        ),
        new_messages: generate_messages_with_ids(successful_cursors)
            .iter()
            .map(Into::into)
            .collect(),
        errored: error_cursors
            .iter()
            .map(|c| (Cursor(*c), GroupMessageProcessingError::InvalidPayload))
            .collect(),
        app_data_changes: Vec::new(),
    });
    summary
}

/// Like `generate_errored_summary`, but every success `MessageIdentifier` shares `group_id`
/// (matches production sync summaries). Random per-message group ids break stream recovery tests.
pub fn generate_errored_summary_with_group(
    group_id: &xmtp_proto::types::GroupId,
    error_cursors: &[u64],
    successful_cursors: &[u64],
) -> SyncSummary {
    let mut summary = SyncSummary::default();
    summary.add_process(ProcessSummary {
        total_messages: HashSet::from_iter(
            error_cursors
                .iter()
                .copied()
                .chain(successful_cursors.iter().copied())
                .map(Cursor),
        ),
        new_messages: successful_cursors
            .iter()
            .map(|c| {
                let m = generate_message(*c, group_id);
                MessageIdentifier::from(&m)
            })
            .collect(),
        errored: error_cursors
            .iter()
            .map(|c| (Cursor(*c), GroupMessageProcessingError::InvalidPayload))
            .collect(),
        app_data_changes: Vec::new(),
    });
    summary
}

pub fn generate_stored_msg(
    cursor: Cursor,
    group_id: xmtp_proto::types::GroupId,
) -> StoredGroupMessage {
    StoredGroupMessage {
        id: xmtp_common::rand_vec::<32>(),
        group_id,
        decrypted_message_bytes: b"test message".to_vec(),
        sent_at_ns: 100,
        kind: xmtp_db::group_message::GroupMessageKind::Application,
        sender_installation_id: xmtp_common::rand_vec::<32>(),
        sender_inbox_id: "test inbox".into(),
        delivery_status: xmtp_db::group_message::DeliveryStatus::Published,
        content_type: xmtp_db::group_message::ContentType::Text,
        version_major: 0,
        version_minor: 0,
        authority_id: "testauthority".to_string(),
        reference_id: None,
        sequence_id: cursor.0 as i64,
        envelope_hash: None,
        expiry_ns: None,
        expiry_unknown: false,
        deleted_by_inbox_id: None,
        expire_at_ns: None,
        inserted_at_ns: 0,
        should_push: true,
        idempotency_key: 100.to_string(),
    }
}
