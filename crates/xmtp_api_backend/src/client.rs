use crate::endpoints::backend;
use arc_swap::ArcSwap;
use std::sync::Arc;
use xmtp_configuration::LimitsConfiguration;
use xmtp_proto::{
    api::{ApiClientError, Client, Query},
    api_client::XmtpBackendClient,
    backend_v1::*,
};

/// Sends backend requests through one transport.
#[derive(Clone, Debug)]
pub struct BackendClient<C> {
    pub(crate) client: C,
    pub(crate) auth_handle: Option<crate::AuthHandle>,
    /// The shapes this deployment accepts. Swapped in once, by
    /// `build`, after the configuration is read and before any stream opens.
    /// The compiled defaults until then, which is what every transport built
    /// without a client keeps.
    pub(crate) limits: Arc<ArcSwap<LimitsConfiguration>>,
}
impl<C> BackendClient<C> {
    pub fn new(client: C) -> Self {
        Self {
            client,
            auth_handle: None,
            limits: Arc::new(ArcSwap::from_pointee(LimitsConfiguration::default())),
        }
    }
    pub fn inner(&self) -> &C {
        &self.client
    }
    pub fn with_auth_handle(mut self, handle: Option<crate::AuthHandle>) -> Self {
        self.auth_handle = handle;
        self
    }
    pub fn auth_handle(&self) -> Option<crate::AuthHandle> {
        self.auth_handle.clone()
    }
    /// What this transport chunks its streams and metadata reads to.
    pub(crate) fn limits(&self) -> arc_swap::Guard<Arc<LimitsConfiguration>> {
        self.limits.load()
    }
}
#[xmtp_common::async_trait]
impl<C: Client> XmtpBackendClient for BackendClient<C> {
    type Error = ApiClientError;
    fn register_client_event_writer(&self, writer: &Arc<dyn xmtp_events::EventWriter<()>>) {
        if let Some(handle) = &self.auth_handle {
            handle.register_event_writer(writer);
        }
    }
    fn unregister_client_event_writer(&self, writer: &Arc<dyn xmtp_events::EventWriter<()>>) {
        if let Some(handle) = &self.auth_handle {
            handle.unregister_event_writer(writer);
        }
    }
    async fn publish(&self, request: PublishRequest) -> Result<PublishResponse, Self::Error> {
        backend::Publish(request).query(&self.client).await
    }
    async fn query(&self, request: QueryRequest) -> Result<QueryResponse, Self::Error> {
        backend::Query(request).query(&self.client).await
    }
    async fn query_newest(
        &self,
        request: QueryNewestRequest,
    ) -> Result<QueryNewestResponse, Self::Error> {
        backend::QueryNewest(request).query(&self.client).await
    }
    async fn get_inbox_ids(
        &self,
        request: GetInboxIdsRequest,
    ) -> Result<GetInboxIdsResponse, Self::Error> {
        backend::GetInboxIds(request).query(&self.client).await
    }
    async fn get_configuration(
        &self,
        request: GetConfigurationRequest,
    ) -> Result<GetConfigurationResponse, Self::Error> {
        backend::GetConfiguration(request).query(&self.client).await
    }
    fn backend_url(&self) -> Option<&str> {
        Some(self.client.host())
    }

    fn has_credential_source(&self) -> bool {
        self.client.has_credential_source()
    }

    fn set_limits(&self, limits: Arc<LimitsConfiguration>) {
        // A zero here would panic `chunks(0)` in `streams.rs`. Wire conversion
        // replaces zeroes; this also does so for a snapshot an app built
        // in Rust and supplied through a `ConfigProvider`.
        self.limits.store(Arc::new(limits.without_zeroes()));
    }
    async fn verify_smart_contract_wallet_signatures(
        &self,
        request: VerifySmartContractWalletSignaturesRequest,
    ) -> Result<VerifySmartContractWalletSignaturesResponse, Self::Error> {
        backend::VerifySmartContractWalletSignatures(request)
            .query(&self.client)
            .await
    }
    async fn register(&self, request: RegisterRequest) -> Result<RecipientState, Self::Error> {
        backend::Register(request).query(&self.client).await
    }
    async fn unregister(
        &self,
        request: UnregisterRequest,
    ) -> Result<UnregisterResponse, Self::Error> {
        backend::Unregister(request).query(&self.client).await
    }
    async fn update_subscriptions(
        &self,
        request: UpdateSubscriptionsRequest,
    ) -> Result<RecipientState, Self::Error> {
        backend::UpdateSubscriptions(request)
            .query(&self.client)
            .await
    }
}

#[xmtp_common::async_trait]
impl<C: xmtp_proto::api::IsConnectedCheck> xmtp_proto::api::IsConnectedCheck for BackendClient<C> {
    async fn is_connected(&self) -> bool {
        self.client.is_connected().await
    }
}
