use crate::endpoints::backend;
use xmtp_proto::{
    api::{ApiClientError, Client, Query},
    api_client::XmtpBackendClient,
    backend_v1::*,
};

/// Sends backend requests through one transport.
#[derive(Clone, Debug)]
pub struct BackendClient<C> {
    pub(crate) client: C,
}
impl<C> BackendClient<C> {
    pub fn new(client: C) -> Self {
        Self { client }
    }
    pub fn inner(&self) -> &C {
        &self.client
    }
}
#[xmtp_common::async_trait]
impl<C: Client> XmtpBackendClient for BackendClient<C> {
    type Error = ApiClientError;
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
    async fn verify_smart_contract_wallet_signatures(
        &self,
        request: VerifySmartContractWalletSignaturesRequest,
    ) -> Result<VerifySmartContractWalletSignaturesResponse, Self::Error> {
        backend::VerifySmartContractWalletSignatures(request)
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
