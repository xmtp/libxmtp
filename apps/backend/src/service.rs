mod attachment;
mod configuration;
mod conversion;
mod error;
mod identity;
pub(crate) mod notification;
mod publish;
mod query;
mod subscription;

pub(crate) use error::publish_invalid;

use crate::{config::ValidatedConfig, db::Store};
use std::sync::Arc;
use xmtp_id::scw_verifier::{CachedSmartContractSignatureVerifier, ChainBlocks};

#[derive(Clone)]
pub struct Backend {
    pub store: Arc<Store>,
    pub config: Arc<ValidatedConfig>,
    pub verifier: Arc<CachedSmartContractSignatureVerifier>,
    /// Chain heads for admission freshness, routed like `verifier`.
    pub chains: Arc<dyn ChainBlocks>,
    pub(crate) auth: Option<Arc<crate::auth::Authentication>>,
    pub(crate) attachments: Option<Arc<dyn xmtp_attachments_server::StorageTarget>>,
    pub(crate) streams: Option<Arc<crate::stream::StreamHub>>,
    pub(crate) push: Option<Arc<crate::push::PushHub>>,
    /// The published deployment settings, built once from the validated
    /// configuration and returned unchanged for the life of the process.
    pub(crate) configuration: Arc<crate::api::GetConfigurationResponse>,
    /// Bounds identity-log replays across all requests.
    pub(crate) replays: Arc<tokio::sync::Semaphore>,
    /// Counts publish-signature verification work for tests.
    #[cfg(test)]
    pub(crate) signature_checks: Arc<publish::SignatureChecks>,
    /// Counts identity-log replays that ran off the thread that started
    /// them, for tests. A replay run inline on the async runtime is not
    /// counted.
    #[cfg(test)]
    pub(crate) identity_replays: Arc<std::sync::atomic::AtomicUsize>,
    /// Replaces the publish admission deadline for tests.
    #[cfg(test)]
    pub(crate) admission_deadline: Arc<parking_lot::Mutex<Option<std::time::Duration>>>,
}

impl Backend {
    /// Build the service state shared by all RPC implementations.
    ///
    /// The configuration, verifier, and chains are shared immutably. The store is cheap
    /// to clone because its pools are reference counted by SQLx. `configuration` is
    /// the published response that `server::initialize` built and size-checked.
    pub fn new(
        store: Store,
        config: ValidatedConfig,
        verifier: CachedSmartContractSignatureVerifier,
        chains: Arc<dyn ChainBlocks>,
        configuration: crate::api::GetConfigurationResponse,
    ) -> Self {
        let configuration = Arc::new(configuration);
        Self {
            store: Arc::new(store),
            config: Arc::new(config),
            verifier: Arc::new(verifier),
            chains,
            streams: None,
            push: None,
            auth: None,
            attachments: None,
            configuration,
            replays: Arc::new(tokio::sync::Semaphore::new(publish::MAX_IDENTITY_REPLAYS)),
            #[cfg(test)]
            signature_checks: Arc::default(),
            #[cfg(test)]
            identity_replays: Arc::default(),
            #[cfg(test)]
            admission_deadline: Arc::default(),
        }
    }
}
