mod conversion;
mod error;
mod identity;
mod publish;
mod query;
mod subscription;

use crate::{config::Config, db::Store};
use std::sync::Arc;
use xmtp_id::scw_verifier::CachedSmartContractSignatureVerifier;

#[derive(Clone)]
pub struct Backend {
    pub store: Store,
    pub config: Arc<Config>,
    pub verifier: Arc<CachedSmartContractSignatureVerifier>,
    pub(crate) streams: Option<Arc<crate::stream::StreamHub>>,
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
        Self {
            store,
            config: Arc::new(config),
            verifier: Arc::new(verifier),
            streams: None,
        }
    }
}
