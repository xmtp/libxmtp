use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use crate::groups::summary::SyncSummary;
use crate::identity::create_credential;
use crate::{
    builder::SyncWorkerMode, client::DeviceSync, context::XmtpMlsLocalContext, identity::Identity,
    mutex_registry::MutexRegistry, subscriptions::LocalEvents, utils::VersionInfo,
};
use ethers::signers::LocalWallet;
use mockall::mock;
use tokio::sync::broadcast;
use xmtp_api::test_utils::MockApiClient;
use xmtp_api::ApiClientWrapper;
use xmtp_cryptography::XmtpInstallationCredential;
use xmtp_id::associations::test_utils::{MockSmartContractSignatureVerifier, WalletTestExt};

mod generate;
pub use generate::*;

pub type MockApiWrapper = Arc<ApiClientWrapper<MockApiClient>>;

pub type MockXmtpLocalContext = XmtpMlsLocalContext<MockApiClient, xmtp_db::MockXmtpDb>;

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
    pub MlsGroup<MockApiClient, MockXmtpDb> {
        fn sync_with_conn(&self) -> Result<SyncSummary, SyncSummary>;

    }
}
/*
mock! {
    pub ProcessMessageFuture<MockApiClient, MockXmtpDb> {
        pub fn apply_or_sync_with_message(&self) -> SyncSummary;
    }
}
*/
