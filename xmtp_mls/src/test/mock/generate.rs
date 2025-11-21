use std::collections::HashSet;

use xmtp_common::{Generate, rand_vec};
use xmtp_db::{MemoryStorage, group_message::StoredGroupMessage, sql_key_store::SqlKeyStore};
use xmtp_proto::types::Cursor;

use crate::{
    context::ClientMode,
    groups::{mls_sync::GroupMessageProcessingError, summary::ProcessSummary},
    tasks::TaskWorkerChannels,
};

use super::*;
use rstest::*;

#[fixture]
pub fn context() -> NewMockContext {
    let (local_events, _) = tokio::sync::broadcast::channel(32);
    let (worker_events, _) = tokio::sync::broadcast::channel(32);
    let (events, _) = tokio::sync::broadcast::channel(32);
    XmtpMlsLocalContext {
        identity: Identity::mock_identity(),
        api_client: ApiClientWrapper::new(MockApiClient::new(), Default::default()),
        store: xmtp_db::MockXmtpDb::new(),
        mutexes: MutexRegistry::new(),
        mls_commit_lock: Default::default(),
        version_info: VersionInfo::default(),
        local_events,
        worker_events,
        events,
        scw_verifier: Arc::new(Box::new(MockSmartContractSignatureVerifier::new(true))),
        device_sync: DeviceSync {
            server_url: None,
            mode: SyncWorkerMode::Disabled,
        },
        fork_recovery_opts: Default::default(),
        mls_storage: SqlKeyStore::new(MemoryStorage::new()),
        sync_api_client: ApiClientWrapper::new(MockApiClient::new(), Default::default()),
        task_channels: TaskWorkerChannels::default(),
        worker_metrics: Arc::default(),
        mode: ClientMode::default(),
    }
}

pub fn generate_inbox_id_credential() -> (String, XmtpInstallationCredential) {
    let signing_key = XmtpInstallationCredential::new();

    let wallet = PrivateKeySigner::random();
    let inbox_id = wallet.identifier().inbox_id(0).unwrap();

    (inbox_id, signing_key)
}

pub fn generate_messages_with_ids(ids: &[u64]) -> Vec<xmtp_proto::types::GroupMessage> {
    ids.iter()
        .map(|id| generate_message(*id, &rand_vec::<16>()))
        .collect()
}

pub fn generate_message(cursor: u64, group_id: &[u8]) -> xmtp_proto::types::GroupMessage {
    let mut msg = xmtp_proto::types::GroupMessage::generate();
    msg.cursor.sequence_id = cursor;
    msg.cursor.originator_id = xmtp_configuration::Originators::APPLICATION_MESSAGES;
    msg.group_id = group_id.into();
    msg
}

pub fn generate_successful_summary(messages: &[xmtp_proto::types::GroupMessage]) -> SyncSummary {
    SyncSummary {
        publish_errors: vec![],
        process: ProcessSummary {
            total_messages: HashSet::from_iter(messages.iter().map(|m| m.cursor)),
            new_messages: messages.iter().map(Into::into).collect(),
            errored: Vec::new(),
        },
        post_commit_errors: vec![],
        other: None,
    }
}

pub fn generate_errored_summary(error_cursors: &[u64], successful_cursors: &[u64]) -> SyncSummary {
    SyncSummary {
        publish_errors: vec![],
        process: ProcessSummary {
            total_messages: HashSet::from_iter(
                error_cursors
                    .iter()
                    .copied()
                    .chain(successful_cursors.iter().copied())
                    .map(|c| Cursor {
                        sequence_id: c,
                        originator_id: xmtp_configuration::Originators::APPLICATION_MESSAGES,
                    }),
            ),
            new_messages: generate_messages_with_ids(successful_cursors)
                .iter()
                .map(Into::into)
                .collect(),
            errored: error_cursors
                .iter()
                .map(|c| {
                    (
                        Cursor::v3_messages(*c),
                        GroupMessageProcessingError::InvalidPayload,
                    )
                })
                .collect(),
        },
        post_commit_errors: vec![],
        other: None,
    }
}

pub fn generate_stored_msg(cursor: Cursor, group_id: Vec<u8>) -> StoredGroupMessage {
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
        sequence_id: cursor.sequence_id as i64,
        originator_id: cursor.originator_id as i64,
        expire_at_ns: None,
        inserted_at_ns: 0,
    }
}
