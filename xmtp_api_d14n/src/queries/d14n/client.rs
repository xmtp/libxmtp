use std::sync::Arc;

use xmtp_id::scw_verifier::MultiSmartContractSignatureVerifier;
use xmtp_id::scw_verifier::VerifierError;
use xmtp_proto::api::IsConnectedCheck;

#[derive(Clone)]
pub struct D14nClient<C, Store> {
    pub(super) client: C,
    pub(super) cursor_store: Store,
    pub(super) scw_verifier: Arc<MultiSmartContractSignatureVerifier>,
}

impl<C, Store> D14nClient<C, Store> {
    pub fn new(client: C, cursor_store: Store) -> Result<Self, VerifierError> {
        Ok(Self {
            client,
            cursor_store,
            scw_verifier: Arc::new(MultiSmartContractSignatureVerifier::new_from_env()?),
        })
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<C, Store> IsConnectedCheck for D14nClient<C, Store>
where
    C: IsConnectedCheck + Send + Sync,
    Store: Send + Sync,
{
    async fn is_connected(&self) -> bool {
        self.client.is_connected().await && self.client.is_connected().await
    }
}
