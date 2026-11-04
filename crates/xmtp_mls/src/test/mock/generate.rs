use std::collections::HashSet;

use xmtp_common::Generate;
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

pub use xmtp_mls_engine::groups::test_support::message_rows::generate_stored_msg;
