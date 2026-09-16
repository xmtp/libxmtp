mod configuration;
mod conversion;
mod error;
mod identity;
pub(crate) mod notification;
mod publish;
mod query;
mod subscription;

use crate::{config::Config, db::Store};
use std::sync::Arc;
use xmtp_id::scw_verifier::CachedSmartContractSignatureVerifier;

#[derive(Clone)]
pub struct Backend {
    pub store: Arc<Store>,
    pub config: Arc<Config>,
    pub verifier: Arc<CachedSmartContractSignatureVerifier>,
    pub(crate) auth: Option<Arc<crate::auth::Authentication>>,
    pub(crate) streams: Option<Arc<crate::stream::StreamHub>>,
    pub(crate) push: Option<Arc<crate::push::PushHub>>,
    /// The published deployment settings, built once from the validated
    /// configuration and returned unchanged for the life of the process.
    pub(crate) configuration: Arc<crate::api::GetConfigurationResponse>,
}

impl Backend {
    /// Build the service state shared by all RPC implementations.
    ///
    /// The configuration and verifier are shared immutably. The store is cheap
    /// to clone because its pools are reference counted by SQLx.
    pub fn new(
        store: Store,
        config: Config,
        verifier: CachedSmartContractSignatureVerifier,
    ) -> Self {
        // A JWKS deployment replaces this in `server::initialize` once its key
        // set has been fetched. Every other deployment publishes it as built.
        let configuration = Arc::new(config.configuration_response(&[]));
        Self {
            store: Arc::new(store),
            config: Arc::new(config),
            verifier: Arc::new(verifier),
            streams: None,
            push: None,
            auth: None,
            configuration,
        }
    }
}
