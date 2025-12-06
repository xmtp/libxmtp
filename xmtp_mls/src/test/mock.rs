use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use crate::builder::ForkRecoveryOpts;
use crate::context::XmtpSharedContext;
use crate::groups::MlsGroup;
use crate::groups::summary::SyncSummary;
use crate::identity::create_credential;
use crate::subscriptions::SubscribeError;
use crate::subscriptions::process_message::{
    ProcessFutureFactory, ProcessMessageFuture, ProcessedMessage,
};
use crate::worker::{MetricsCasting, WorkerKind};
use crate::{
    builder::SyncWorkerMode, client::DeviceSync, context::XmtpMlsLocalContext, identity::Identity,
    mutex_registry::MutexRegistry, utils::VersionInfo,
};
use alloy::signers::local::PrivateKeySigner;
use mockall::mock;
use tokio::sync::broadcast;
use xmtp_api::ApiClientWrapper;
use xmtp_api_d14n::MockApiClient;
use xmtp_cryptography::XmtpInstallationCredential;
use xmtp_db::XmtpDb;
use xmtp_db::sql_key_store::mock::MockSqlKeyStore;
use xmtp_id::associations::test_utils::{MockSmartContractSignatureVerifier, WalletTestExt};
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;

mod generate;
pub use generate::*;
mod openmls_mock;
pub use openmls_mock::*;

pub type MockApiWrapper = Arc<ApiClientWrapper<MockApiClient>>;
pub type MockStoreAndContext =
    XmtpMlsLocalContext<MockApiClient, xmtp_db::MockXmtpDb, MockSqlKeyStore>;
pub type MockContext = Arc<
    XmtpMlsLocalContext<MockApiClient, xmtp_db::MockXmtpDb, xmtp_db::test_utils::MlsMemoryStorage>,
>;
/// A mock context type that hasn't yet been added into an Arc type.
pub type NewMockContext =
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
        fn create(&self, msg: xmtp_proto::types::GroupMessage) -> xmtp_common::BoxDynFuture<'_, Result<ProcessedMessage, SubscribeError>>;
        fn retrieve(&self, msg: &xmtp_proto::types::GroupMessage) -> Result<Option<xmtp_db::group_message::StoredGroupMessage>, SubscribeError>;
    }
}

mock! {
    pub MockMlsGroup {
        fn sync_with_conn(&self) -> Result<SyncSummary, SyncSummary>;
    }
}

impl Clone for NewMockContext {
    fn clone(&self) -> Self {
        Self {
            identity: self.identity.clone(),
            api_client: self.api_client.clone(),
            sync_api_client: self.sync_api_client.clone(),
            store: self.store.clone(),
            mls_storage: self.mls_storage.clone(),
            mutexes: self.mutexes.clone(),
            mls_commit_lock: self.mls_commit_lock.clone(),
            version_info: self.version_info.clone(),
            local_events: self.local_events.clone(),
            worker_events: self.worker_events.clone(),
            scw_verifier: self.scw_verifier.clone(),
            device_sync: self.device_sync.clone(),
            fork_recovery_opts: self.fork_recovery_opts.clone(),
            task_channels: self.task_channels.clone(),
            worker_metrics: self.worker_metrics.clone(),
        }
    }
}

impl XmtpSharedContext for NewMockContext {
    type Db = xmtp_db::MockXmtpDb;

    type ApiClient = MockApiClient;

    type MlsStorage = xmtp_db::test_utils::MlsMemoryStorage;
    type ContextReference = Self;

    fn context_ref(&self) -> &Self::ContextReference {
        self
    }

    fn db(&self) -> <Self::Db as xmtp_db::XmtpDb>::DbQuery {
        self.store.db()
    }

    fn api(&self) -> &ApiClientWrapper<Self::ApiClient> {
        &self.api_client
    }

    fn scw_verifier(&self) -> Arc<Box<dyn SmartContractSignatureVerifier>> {
        self.scw_verifier.clone()
    }

    fn device_sync(&self) -> &DeviceSync {
        &self.device_sync
    }

    fn fork_recovery_opts(&self) -> &ForkRecoveryOpts {
        &self.fork_recovery_opts
    }

    fn mls_storage(&self) -> &Self::MlsStorage {
        &self.mls_storage
    }

    fn identity(&self) -> &Identity {
        &self.identity
    }

    fn version_info(&self) -> &VersionInfo {
        &self.version_info
    }

    fn worker_events(&self) -> &broadcast::Sender<crate::subscriptions::SyncWorkerEvent> {
        &self.worker_events
    }

    fn local_events(&self) -> &broadcast::Sender<crate::subscriptions::LocalEvents> {
        &self.local_events
    }

    fn mls_commit_lock(&self) -> &Arc<crate::GroupCommitLock> {
        &self.mls_commit_lock
    }

    fn mutexes(&self) -> &MutexRegistry {
        &self.mutexes
    }

    fn task_channels(&self) -> &crate::tasks::TaskWorkerChannels {
        &self.task_channels
    }

    fn sync_metrics(
        &self,
    ) -> Option<
        Arc<crate::worker::metrics::WorkerMetrics<crate::groups::device_sync::worker::SyncMetric>>,
    > {
        self.worker_metrics
            .lock()
            .get(&WorkerKind::DeviceSync)?
            .as_sync_metrics()
    }

    fn sync_api(&self) -> &ApiClientWrapper<Self::ApiClient> {
        &self.sync_api_client
    }
}
