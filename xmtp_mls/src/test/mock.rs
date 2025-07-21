use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use crate::context::XmtpSharedContext;
use crate::groups::summary::SyncSummary;
use crate::groups::MlsGroup;
use crate::identity::create_credential;
use crate::subscriptions::process_message::{
    ProcessFutureFactory, ProcessMessageFuture, ProcessedMessage,
};
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
use tokio::sync::broadcast;
use xmtp_id::InboxIdRef;
use xmtp_db::XmtpOpenMlsProviderRef;
use xmtp_common::types::InstallationId;
use xmtp_id::associations::builder::SignatureRequest;
use crate::subscriptions::SubscribeError;
use crate::{
    builder::SyncWorkerMode, client::DeviceSync, context::XmtpMlsLocalContext, identity::Identity,
    mutex_registry::MutexRegistry, utils::VersionInfo,
};
use alloy::signers::local::PrivateKeySigner;
use mockall::mock;
use xmtp_api::test_utils::MockApiClient;
use xmtp_api::ApiClientWrapper;
use xmtp_cryptography::XmtpInstallationCredential;
use xmtp_id::associations::test_utils::{MockSmartContractSignatureVerifier, WalletTestExt};

mod generate;
pub use generate::*;

pub type MockApiWrapper = Arc<ApiClientWrapper<MockApiClient>>;
pub type MockContext =
    XmtpMlsLocalContext<MockApiClient, xmtp_db::MockXmtpDb, xmtp_db::test_utils::MlsMemoryStorage>;
pub type MockProcessMessageFuture = ProcessMessageFuture<MockContext>;
pub type MockMlsGroup = MlsGroup<MockContext>;


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

impl Clone for MockContext {
    fn clone(&self) -> Self {
        Self {
            identity: self.identity.clone(),
            api_client: self.api_client.clone(),
            store: self.store.clone(),
            mls_storage: self.mls_storage.clone(),
            mutexes: self.mutexes.clone(),
            mls_commit_lock: self.mls_commit_lock.clone(),
            version_info: self.version_info.clone(),
            local_events: self.local_events.clone(),
            worker_events: self.worker_events.clone(),
            scw_verifier: self.scw_verifier.clone(),
            device_sync: self.device_sync.clone(),
            workers: self.workers.clone()
        }
    }
}

impl XmtpSharedContext for MockContext {
    type Db = xmtp_db::MockXmtpDb;

    type ApiClient = MockApiClient;

    type MlsStorage = xmtp_db::test_utils::MlsMemoryStorage;
    type ContextReference = Self;

    fn context_ref(
        &self,
    ) -> &Self::ContextReference {
        todo!()
    }

    fn db(&self) -> <Self::Db as xmtp_db::XmtpDb>::DbQuery {
        todo!()
    }

    fn api(&self) -> &ApiClientWrapper<Self::ApiClient> {
        todo!()
    }

    fn scw_verifier(&self) -> Arc<Box<dyn SmartContractSignatureVerifier>> {
        todo!()
    }

    fn device_sync(&self) -> &DeviceSync {
        todo!()
    }

    fn device_sync_server_url(&self) -> Option<&String> {
        todo!()
    }

    fn device_sync_worker_enabled(&self) -> bool {
        todo!()
    }

    fn mls_provider(&self) -> XmtpOpenMlsProviderRef<Self::MlsStorage> {
        todo!()
    }

    fn mls_storage(&self) -> &Self::MlsStorage {
        todo!()
    }

    fn signature_request(&self) -> Option<SignatureRequest> {
        todo!()
    }

    fn identity(&self) -> &Identity {
        todo!()
    }

    fn inbox_id(&self) -> InboxIdRef<'_> {
        todo!()
    }

    fn installation_id(&self) -> InstallationId {
        todo!()
    }

    fn version_info(&self) -> &VersionInfo {
        todo!()
    }

    fn worker_events(&self) -> &broadcast::Sender<crate::subscriptions::SyncWorkerEvent> {
        todo!()
    }

    fn local_events(&self) -> &broadcast::Sender<crate::subscriptions::LocalEvents> {
        todo!()
    }

    fn mls_commit_lock(&self) -> &Arc<crate::GroupCommitLock> {
        todo!()
    }

    fn workers(&self) -> &crate::worker::WorkerRunner {
        todo!()
    }

    fn mutexes(&self) -> &MutexRegistry {
        todo!()
    }
}
