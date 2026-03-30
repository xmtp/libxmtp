use std::sync::Arc;

use xmtp_common::MaybeSend;
use xmtp_common::MaybeSync;
use xmtp_id::scw_verifier::MultiSmartContractSignatureVerifier;
use xmtp_id::scw_verifier::VerifierError;
use xmtp_proto::api::IsConnectedCheck;

#[derive(Clone)]
pub struct D14nClient<C, Store> {
    pub(super) client: C,
    pub(super) cursor_store: Store,
    pub(super) scw_verifier: Arc<MultiSmartContractSignatureVerifier>,
    /// App version for per-node gRPC clients (ensures visibility-check requests carry metrics).
    pub(super) app_version: Option<xmtp_proto::types::AppVersion>,
}

impl<C: std::fmt::Debug, Store> std::fmt::Debug for D14nClient<C, Store> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("D14nClient")
            .field("client", &self.client)
            .field("scw_verifier", &self.scw_verifier)
            .finish()
    }
}

impl<C, Store> D14nClient<C, Store> {
    pub fn new(client: C, cursor_store: Store) -> Result<Self, VerifierError> {
        Ok(Self {
            client,
            cursor_store,
            scw_verifier: Arc::new(MultiSmartContractSignatureVerifier::new_from_env()?),
            app_version: None,
        })
    }

    pub fn with_app_version(mut self, version: xmtp_proto::types::AppVersion) -> Self {
        self.app_version = Some(version);
        self
    }
}
xmtp_common::if_test! {
    use xmtp_proto::api::mock::MockNetworkClient;
    use crate::protocol::NoCursorStore;

    impl crate::MockD14nClient {
        pub fn new_mock() -> Self {
            Self {
                client: MockNetworkClient::new(),
                cursor_store: NoCursorStore,
                scw_verifier: Arc::new(MultiSmartContractSignatureVerifier::new_from_env().expect("scw failed")),
                app_version: None,
            }
        }
    }
    impl<S> D14nClient<MockNetworkClient, S> {
        pub fn new_mock_with_store(store: S) -> Self {
        Self {
            client: MockNetworkClient::new(),
            cursor_store: store,
            scw_verifier: Arc::new(MultiSmartContractSignatureVerifier::new_from_env().expect("scw failed")),
            app_version: None,
        }
    }
}

}
#[xmtp_common::async_trait]
impl<C, Store> IsConnectedCheck for D14nClient<C, Store>
where
    C: IsConnectedCheck,
    Store: MaybeSend + MaybeSync,
{
    async fn is_connected(&self) -> bool {
        self.client.is_connected().await
    }
}
