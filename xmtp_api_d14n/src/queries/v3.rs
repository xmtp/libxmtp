use crate::v3::*;
use futures::stream;
use xmtp_common::RetryableError;
use xmtp_proto::api::{ApiClientError, Client, Query};
use xmtp_proto::api_client::{
    ApiStats, IdentityStats, XmtpIdentityClient, XmtpMlsClient, XmtpMlsStreams,
};
use xmtp_proto::identity_v1;
use xmtp_proto::mls_v1;
use xmtp_proto::prelude::ApiBuilder;
use xmtp_proto::types::AppVersion;
use xmtp_proto::xmtp::identity::associations::IdentifierKind;

#[derive(Clone)]
pub struct V3Client<C> {
    client: C,
}

impl<C> V3Client<C> {
    pub fn new(client: C) -> Self {
        Self { client }
    }
}

pub struct V3ClientBuilder<Builder> {
    client: Builder,
}

impl<Builder> V3ClientBuilder<Builder> {
    pub fn new(client: Builder) -> Self {
        Self { client }
    }
}

impl<Builder> ApiBuilder for V3ClientBuilder<Builder>
where
    Builder: ApiBuilder,
    <Builder as ApiBuilder>::Output: xmtp_proto::api::Client,
{
    type Output = V3Client<<Builder as ApiBuilder>::Output>;

    type Error = <Builder as ApiBuilder>::Error;

    fn set_libxmtp_version(&mut self, version: String) -> Result<(), Self::Error> {
        <Builder as ApiBuilder>::set_libxmtp_version(&mut self.client, version)
    }
    fn set_app_version(&mut self, version: AppVersion) -> Result<(), Self::Error> {
        <Builder as ApiBuilder>::set_app_version(&mut self.client, version)
    }

    fn set_host(&mut self, host: String) {
        <Builder as ApiBuilder>::set_host(&mut self.client, host)
    }

    fn set_tls(&mut self, tls: bool) {
        <Builder as ApiBuilder>::set_tls(&mut self.client, tls)
    }

    fn set_retry(&mut self, retry: xmtp_common::Retry) {
        <Builder as ApiBuilder>::set_retry(&mut self.client, retry)
    }

    fn rate_per_minute(&mut self, limit: u32) {
        <Builder as ApiBuilder>::rate_per_minute(&mut self.client, limit)
    }

    fn port(&self) -> Result<Option<String>, Self::Error> {
        <Builder as ApiBuilder>::port(&self.client)
    }

    fn host(&self) -> Option<&str> {
        <Builder as ApiBuilder>::host(&self.client)
    }

    async fn build(self) -> Result<Self::Output, Self::Error> {
        Ok(V3Client::new(
            <Builder as ApiBuilder>::build(self.client).await?,
        ))
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<C, E> XmtpMlsClient for V3Client<C>
where
    E: std::error::Error + RetryableError + Send + Sync + 'static,
    C: Send + Sync + Client<Error = E>,
    ApiClientError<E>: From<ApiClientError<<C as Client>::Error>> + Send + Sync + 'static,
{
    type Error = ApiClientError<E>;

    async fn upload_key_package(
        &self,
        request: mls_v1::UploadKeyPackageRequest,
    ) -> Result<(), Self::Error> {
        UploadKeyPackage::builder()
            .key_package(request.key_package)
            .is_inbox_id_credential(request.is_inbox_id_credential)
            .build()?
            .query(&self.client)
            .await
    }
    async fn fetch_key_packages(
        &self,
        request: mls_v1::FetchKeyPackagesRequest,
    ) -> Result<mls_v1::FetchKeyPackagesResponse, Self::Error> {
        FetchKeyPackages::builder()
            .installation_keys(request.installation_keys)
            .build()?
            .query(&self.client)
            .await
    }
    async fn send_group_messages(
        &self,
        request: mls_v1::SendGroupMessagesRequest,
    ) -> Result<(), Self::Error> {
        SendGroupMessages::builder()
            .messages(request.messages)
            .build()?
            .query(&self.client)
            .await
    }
    async fn send_welcome_messages(
        &self,
        request: mls_v1::SendWelcomeMessagesRequest,
    ) -> Result<(), Self::Error> {
        SendWelcomeMessages::builder()
            .messages(request.messages)
            .build()?
            .query(&self.client)
            .await
    }
    async fn query_group_messages(
        &self,
        request: mls_v1::QueryGroupMessagesRequest,
    ) -> Result<mls_v1::QueryGroupMessagesResponse, Self::Error> {
        QueryGroupMessages::builder()
            .group_id(request.group_id)
            .build()?
            .query(&self.client)
            .await
    }
    async fn query_welcome_messages(
        &self,
        request: mls_v1::QueryWelcomeMessagesRequest,
    ) -> Result<mls_v1::QueryWelcomeMessagesResponse, Self::Error> {
        QueryWelcomeMessages::builder()
            .installation_key(request.installation_key)
            .paging_info(request.paging_info)
            .build()?
            .query(&self.client)
            .await
    }

    async fn publish_commit_log(
        &self,
        request: mls_v1::BatchPublishCommitLogRequest,
    ) -> Result<(), Self::Error> {
        PublishCommitLog::builder()
            .commit_log_entries(request.requests)
            .build()?
            .query(&self.client)
            .await
    }

    async fn query_commit_log(
        &self,
        request: mls_v1::BatchQueryCommitLogRequest,
    ) -> Result<mls_v1::BatchQueryCommitLogResponse, Self::Error> {
        QueryCommitLog::builder()
            .query_log_requests(request.requests)
            .build()?
            .query(&self.client)
            .await
    }

    fn stats(&self) -> ApiStats {
        Default::default()
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<C, E> XmtpIdentityClient for V3Client<C>
where
    C: Send + Sync + Client<Error = E>,
    E: std::error::Error + RetryableError + Send + Sync + 'static,
{
    type Error = ApiClientError<E>;

    async fn publish_identity_update(
        &self,
        request: identity_v1::PublishIdentityUpdateRequest,
    ) -> Result<identity_v1::PublishIdentityUpdateResponse, Self::Error> {
        PublishIdentityUpdate::builder()
            //todo: handle error or tryFrom
            .identity_update(request.identity_update)
            .build()?
            .query(&self.client)
            .await
    }

    async fn get_identity_updates_v2(
        &self,
        request: identity_v1::GetIdentityUpdatesRequest,
    ) -> Result<identity_v1::GetIdentityUpdatesResponse, Self::Error> {
        GetIdentityUpdatesV2::builder()
            .requests(request.requests)
            .build()?
            .query(&self.client)
            .await
    }

    async fn get_inbox_ids(
        &self,
        request: identity_v1::GetInboxIdsRequest,
    ) -> Result<identity_v1::GetInboxIdsResponse, Self::Error> {
        GetInboxIds::builder()
            .addresses(
                request
                    .requests
                    .iter()
                    .filter(|r| r.identifier_kind == IdentifierKind::Ethereum as i32)
                    .map(|r| r.identifier.clone())
                    .collect::<Vec<_>>(),
            )
            .passkeys(
                request
                    .requests
                    .iter()
                    .filter(|r| r.identifier_kind == IdentifierKind::Passkey as i32)
                    .map(|r| r.identifier.clone())
                    .collect::<Vec<_>>(),
            )
            .build()?
            .query(&self.client)
            .await
    }

    async fn verify_smart_contract_wallet_signatures(
        &self,
        request: identity_v1::VerifySmartContractWalletSignaturesRequest,
    ) -> Result<identity_v1::VerifySmartContractWalletSignaturesResponse, Self::Error> {
        VerifySmartContractWalletSignatures::builder()
            .signatures(request.signatures)
            .build()?
            .query(&self.client)
            .await
    }

    fn identity_stats(&self) -> IdentityStats {
        Default::default()
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<C, E> XmtpMlsStreams for V3Client<C>
where
    C: Send + Sync + Client<Error = E>,
    E: std::error::Error + RetryableError + Send + Sync + 'static,
{
    type Error = ApiClientError<E>;

    #[cfg(not(target_arch = "wasm32"))]
    type GroupMessageStream = stream::BoxStream<'static, Result<mls_v1::GroupMessage, Self::Error>>;
    #[cfg(not(target_arch = "wasm32"))]
    type WelcomeMessageStream =
        stream::BoxStream<'static, Result<mls_v1::WelcomeMessage, Self::Error>>;

    #[cfg(target_arch = "wasm32")]
    type GroupMessageStream =
        stream::LocalBoxStream<'static, Result<mls_v1::GroupMessage, Self::Error>>;
    #[cfg(target_arch = "wasm32")]
    type WelcomeMessageStream =
        stream::LocalBoxStream<'static, Result<mls_v1::WelcomeMessage, Self::Error>>;

    async fn subscribe_group_messages(
        &self,
        _request: mls_v1::SubscribeGroupMessagesRequest,
    ) -> Result<Self::GroupMessageStream, Self::Error> {
        todo!()
    }

    async fn subscribe_welcome_messages(
        &self,
        _request: mls_v1::SubscribeWelcomeMessagesRequest,
    ) -> Result<Self::WelcomeMessageStream, Self::Error> {
        todo!()
    }
}
