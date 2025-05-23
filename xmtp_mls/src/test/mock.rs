use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use crate::groups::summary::SyncSummary;
use crate::groups::MlsGroup;
use crate::identity::create_credential;
use crate::subscriptions::process_message::{
    ProcessFutureFactory, ProcessMessageFuture, ProcessedMessage,
};
use crate::subscriptions::SubscribeError;
use crate::{
    builder::SyncWorkerMode, client::DeviceSync, context::XmtpMlsLocalContext, identity::Identity,
    mutex_registry::MutexRegistry, utils::VersionInfo,
};
use ethers::signers::LocalWallet;
use mockall::mock;
use xmtp_api::test_utils::MockApiClient;
use xmtp_api::ApiClientWrapper;
use xmtp_cryptography::XmtpInstallationCredential;
use xmtp_id::associations::test_utils::{MockSmartContractSignatureVerifier, WalletTestExt};

mod generate;
pub use generate::*;

pub type MockApiWrapper = Arc<ApiClientWrapper<MockApiClient>>;
pub type MockContext = XmtpMlsLocalContext<MockApiClient, xmtp_db::MockXmtpDb>;
pub type MockProcessMessageFuture = ProcessMessageFuture<MockApiClient, xmtp_db::MockXmtpDb>;
pub type MockMlsGroup = MlsGroup<MockApiClient, xmtp_db::MockXmtpDb>;

impl Identity {
    pub fn mock_identity() -> Identity {
        let (inbox, cred) = generate_inbox_id_credential();
        Identity {
            inbox_id: inbox.clone(),
            installation_keys: cred,
            credential: create_credential(inbox).unwrap(),
            signature_request: None,
            is_ready: AtomicBool::new(true),
        }
    }
}

mock! {
    pub ProcessFutureFactory {}
    impl ProcessFutureFactory<'_> for ProcessFutureFactory {
        fn create(&self, msg: xmtp_proto::mls_v1::group_message::V1) -> xmtp_common::FutureWrapper<'_, Result<ProcessedMessage, SubscribeError>>;
        fn retrieve(&self, msg: &xmtp_proto::mls_v1::group_message::V1) -> Result<Option<xmtp_db::group_message::StoredGroupMessage>, SubscribeError>;
    }
}

mock! {
    pub MockMlsGroup {
        fn sync_with_conn(&self) -> Result<SyncSummary, SyncSummary>;
    }
}

mock! {
    pub MockContext {
        pub fn inbox_id(&self) -> String;
    }
}
