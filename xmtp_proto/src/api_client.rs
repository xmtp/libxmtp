pub use super::xmtp::message_api::v1::{
    BatchQueryRequest, BatchQueryResponse, Envelope, PublishRequest, PublishResponse, QueryRequest,
    QueryResponse, SubscribeRequest,
};
use crate::mls_v1::{
    BatchPublishCommitLogRequest, BatchQueryCommitLogRequest, BatchQueryCommitLogResponse,
    PagingInfo,
};
use crate::types::{Cursor, GroupMessage, InstallationId, WelcomeMessage};
use crate::xmtp::identity::api::v1::{
    GetIdentityUpdatesRequest as GetIdentityUpdatesV2Request,
    GetIdentityUpdatesResponse as GetIdentityUpdatesV2Response, GetInboxIdsRequest,
    GetInboxIdsResponse, PublishIdentityUpdateRequest, PublishIdentityUpdateResponse,
    VerifySmartContractWalletSignaturesRequest, VerifySmartContractWalletSignaturesResponse,
};
use crate::xmtp::mls::api::v1::{
    FetchKeyPackagesRequest, FetchKeyPackagesResponse, GroupMessage as ProtoGroupMessage,
    QueryWelcomeMessagesResponse, SendGroupMessagesRequest, SendWelcomeMessagesRequest,
    SubscribeGroupMessagesRequest, SubscribeWelcomeMessagesRequest, UploadKeyPackageRequest,
    WelcomeMessage as ProtoWelcomeMessage,
};
use futures::Stream;
use std::sync::Arc;
use xmtp_common::MaybeSend;
use xmtp_common::{Retry, RetryableError};

mod impls;
mod stats;
pub use stats::*;

#[cfg(any(test, feature = "test-utils"))]
pub mod tests;

#[cfg(any(test, feature = "test-utils"))]
pub trait XmtpTestClient {
    type Builder: ApiBuilder;
    fn create_local() -> Self::Builder;
    fn create_d14n() -> Self::Builder;
    fn create_gateway() -> Self::Builder;
    fn create_dev() -> Self::Builder;
}

pub type BoxedXmtpApi<Error> = Box<dyn BoxableXmtpApi<Error>>;
pub type ArcedXmtpApi<Error> = Arc<dyn BoxableXmtpApi<Error>>;

/// XMTP Api Super Trait
/// Implements all Trait Network APIs for convenience.
pub trait BoxableXmtpApi<Err>
where
    Self: XmtpMlsClient<Error = Err> + XmtpIdentityClient<Error = Err> + Send + Sync,
{
}

impl<T, Err> BoxableXmtpApi<Err> for T where
    T: XmtpMlsClient<Error = Err> + XmtpIdentityClient<Error = Err> + Send + Sync + ?Sized
{
}

pub trait XmtpApi
where
    Self: XmtpMlsClient + XmtpIdentityClient + Send + Sync,
{
}

impl<T> XmtpApi for T where T: XmtpMlsClient + XmtpIdentityClient + Send + Sync {}

pub trait Paged {
    type Message;
    fn info(&self) -> &Option<PagingInfo>;
    fn messages(self) -> Vec<Self::Message>;
}

// Wasm futures don't have `Send` or `Sync` bounds.
#[allow(async_fn_in_trait)]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait XmtpMlsClient {
    type Error: RetryableError + Send + Sync + 'static;
    async fn upload_key_package(&self, request: UploadKeyPackageRequest)
    -> Result<(), Self::Error>;
    async fn fetch_key_packages(
        &self,
        request: FetchKeyPackagesRequest,
    ) -> Result<FetchKeyPackagesResponse, Self::Error>;
    async fn send_group_messages(
        &self,
        request: SendGroupMessagesRequest,
    ) -> Result<(), Self::Error>;
    async fn send_welcome_messages(
        &self,
        request: SendWelcomeMessagesRequest,
    ) -> Result<(), Self::Error>;
    async fn query_group_messages(
        &self,
        group_id: crate::types::GroupId,
        cursor: Vec<Cursor>,
    ) -> Result<Vec<GroupMessage>, Self::Error>;
    async fn query_latest_group_message(
        &self,
        group_id: crate::types::GroupId,
    ) -> Result<Option<GroupMessage>, Self::Error>;
    async fn query_welcome_messages(
        &self,
        installation_key: InstallationId,
        cursor: Vec<Cursor>,
    ) -> Result<Vec<WelcomeMessage>, Self::Error>;
    async fn publish_commit_log(
        &self,
        request: BatchPublishCommitLogRequest,
    ) -> Result<(), Self::Error>;
    async fn query_commit_log(
        &self,
        request: BatchQueryCommitLogRequest,
    ) -> Result<BatchQueryCommitLogResponse, Self::Error>;
    fn stats(&self) -> ApiStats;
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait XmtpMlsStreams {
    type GroupMessageStream: Stream<Item = Result<GroupMessage, Self::Error>> + MaybeSend;

    type WelcomeMessageStream: Stream<Item = Result<WelcomeMessage, Self::Error>> + MaybeSend;

    type Error: RetryableError + Send + Sync + 'static;

    async fn subscribe_group_messages(
        &self,
        request: SubscribeGroupMessagesRequest,
    ) -> Result<Self::GroupMessageStream, Self::Error>;
    async fn subscribe_welcome_messages(
        &self,
        request: SubscribeWelcomeMessagesRequest,
    ) -> Result<Self::WelcomeMessageStream, Self::Error>;
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait XmtpIdentityClient {
    type Error: RetryableError + Send + Sync + 'static;
    async fn publish_identity_update(
        &self,
        request: PublishIdentityUpdateRequest,
    ) -> Result<PublishIdentityUpdateResponse, Self::Error>;

    async fn get_identity_updates_v2(
        &self,
        request: GetIdentityUpdatesV2Request,
    ) -> Result<GetIdentityUpdatesV2Response, Self::Error>;

    async fn get_inbox_ids(
        &self,
        request: GetInboxIdsRequest,
    ) -> Result<GetInboxIdsResponse, Self::Error>;

    async fn verify_smart_contract_wallet_signatures(
        &self,
        request: VerifySmartContractWalletSignaturesRequest,
    ) -> Result<VerifySmartContractWalletSignaturesResponse, Self::Error>;

    fn identity_stats(&self) -> IdentityStats;
}

pub trait ApiBuilder {
    type Output;
    type Error;

    /// set the libxmtp version (required)
    fn set_libxmtp_version(&mut self, version: String) -> Result<(), Self::Error>;

    /// set the sdk app version (required)
    fn set_app_version(&mut self, version: crate::types::AppVersion) -> Result<(), Self::Error>;

    /// Set the libxmtp host (required)
    fn set_host(&mut self, host: String);

    /// Set the payer URL (optional)
    fn set_gateway(&mut self, _host: String) {}

    /// indicate tls (default: false)
    fn set_tls(&mut self, tls: bool);

    /// Set the retry strategy for this client
    fn set_retry(&mut self, retry: Retry);

    /// Set the rate limit per minute for this client
    fn rate_per_minute(&mut self, limit: u32);

    /// The port this api builder is using
    fn port(&self) -> Result<Option<String>, Self::Error>;

    /// Host of the builder
    fn host(&self) -> Option<&str>;

    #[allow(async_fn_in_trait)]
    /// Build the api client
    async fn build(self) -> Result<Self::Output, Self::Error>;
}
