mod attachment;
mod configuration;
mod conversion;
mod error;
mod identity;
pub(crate) mod notification;
mod publish;
mod query;
mod subscription;

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
        }
    }
}
