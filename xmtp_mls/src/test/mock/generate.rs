use std::collections::HashSet;

use xmtp_db::group_message::StoredGroupMessage;
use xmtp_proto::{mls_v1::group_message, xmtp::mls::api::v1};

use crate::groups::{mls_sync::GroupMessageProcessingError, summary::ProcessSummary};

use super::*;
use rstest::*;

#[fixture]
pub fn context() -> MockContext {
    let (tx, _) = tokio::sync::broadcast::channel(32);
    MockContext {
        identity: Identity::mock_identity(),
        api_client: ApiClientWrapper::new(MockApiClient::new(), Default::default()),
        store: xmtp_db::MockXmtpDb::new(),
        mutexes: MutexRegistry::new(),
        mls_commit_lock: Default::default(),
        version_info: VersionInfo::default(),
        local_events: tx,
        scw_verifier: Arc::new(Box::new(MockSmartContractSignatureVerifier::new(true))),
        device_sync: DeviceSync {
            server_url: None,
            mode: SyncWorkerMode::Disabled,
            worker_handle: Default::default(),
        },
    }
}

pub fn generate_inbox_id_credential() -> (String, XmtpInstallationCredential) {
    let signing_key = XmtpInstallationCredential::new();

    let wallet = LocalWallet::new(&mut rand::thread_rng());
    let inbox_id = wallet.identifier().inbox_id(0).unwrap();

    (inbox_id, signing_key)
}

pub fn generate_messages_with_ids(ids: &[u64]) -> Vec<group_message::V1> {
    ids.iter().map(|id| generate_message_v1(*id)).collect()
}
pub fn generate_message_v1(cursor: u64) -> group_message::V1 {
    group_message::V1 {
        id: cursor,
        created_ns: xmtp_common::rand_u64(),
        group_id: xmtp_common::rand_vec::<32>(),
        data: b"test data".to_vec(),
        sender_hmac: xmtp_common::rand_vec::<32>(),
        should_push: false,
    }
}

pub fn generate_message(cursor: u64, group_id: &[u8]) -> v1::GroupMessage {
    v1::GroupMessage {
        version: Some(group_message::Version::V1(group_message::V1 {
            id: cursor,
            created_ns: xmtp_common::rand_u64(),
            group_id: group_id.to_vec(),
            data: b"test data".to_vec(),
            sender_hmac: xmtp_common::rand_vec::<32>(),
            should_push: false,
        })),
    }
}

pub fn generate_message_and_v1(
    cursor: u64,
    group_id: &[u8],
) -> (v1::GroupMessage, group_message::V1) {
    let m = group_message::V1 {
        id: cursor,
        created_ns: xmtp_common::rand_u64(),
        group_id: group_id.to_vec(),
        data: b"test data".to_vec(),
        sender_hmac: xmtp_common::rand_vec::<32>(),
        should_push: false,
    };

    (
        v1::GroupMessage {
            version: Some(group_message::Version::V1(m.clone())),
        },
        m,
    )
}

pub fn generate_successful_summary(messages: &[group_message::V1]) -> SyncSummary {
    SyncSummary {
        publish_errors: vec![],
        process: ProcessSummary {
            total_messages: HashSet::from_iter(messages.iter().map(|m| m.id)),
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
                    .chain(successful_cursors.iter().copied()),
            ),
            new_messages: generate_messages_with_ids(successful_cursors)
                .iter()
                .map(Into::into)
                .collect(),
            errored: error_cursors
                .iter()
                .map(|c| (*c, GroupMessageProcessingError::InvalidPayload))
                .collect(),
        },
        post_commit_errors: vec![],
        other: None,
    }
}

pub fn generate_stored_msg(id: u64, group_id: Vec<u8>) -> StoredGroupMessage {
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
        sequence_id: Some(id as i64),
        originator_id: Some(100),
    }
}
