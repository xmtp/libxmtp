use xmtp_proto::mls_v1::group_message;

use super::*;

pub fn generate_mock_context(events: broadcast::Sender<LocalEvents>) -> MockXmtpLocalContext {
    MockXmtpLocalContext {
        identity: Identity::mock_identity(),
        api_client: Arc::new(ApiClientWrapper::new(
            MockApiClient::new(),
            Default::default(),
        )),
        store: xmtp_db::MockXmtpDb::new(),
        mutexes: MutexRegistry::new(),
        mls_commit_lock: Default::default(),
        version_info: VersionInfo::default(),
        local_events: events,
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

pub fn generate_messages_with_ids(ids: Vec<u64>) -> Vec<group_message::V1> {
    ids.into_iter().map(|id| generate_message_v1(id)).collect()
}
pub fn generate_message_v1(cursor: u64) -> group_message::V1 {
    group_message::V1 {
        id: cursor,
        created_ns: xmtp_common::rand_u64(),
        group_id: xmtp_common::rand_vec::<32>(),
        data: xmtp_common::rand_vec::<256>(),
        sender_hmac: xmtp_common::rand_vec::<32>(),
        should_push: false,
    }
}
