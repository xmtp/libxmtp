pub use super::xmtp::message_api::v1::{
    BatchQueryRequest, BatchQueryResponse, Envelope, PagingInfo, PublishRequest, PublishResponse,
    QueryRequest, QueryResponse, SubscribeRequest,
};
use crate::xmtp::identity::api::v1::{
    GetIdentityUpdatesRequest as GetIdentityUpdatesV2Request,
    GetIdentityUpdatesResponse as GetIdentityUpdatesV2Response, GetInboxIdsRequest,
    GetInboxIdsResponse, PublishIdentityUpdateRequest, PublishIdentityUpdateResponse,
    VerifySmartContractWalletSignaturesRequest, VerifySmartContractWalletSignaturesResponse,
};
use crate::xmtp::mls::api::v1::{
    FetchKeyPackagesRequest, FetchKeyPackagesResponse, GroupMessage, QueryGroupMessagesRequest,
    QueryGroupMessagesResponse, QueryWelcomeMessagesRequest, QueryWelcomeMessagesResponse,
    SendGroupMessagesRequest, SendWelcomeMessagesRequest, SubscribeGroupMessagesRequest,
    SubscribeWelcomeMessagesRequest, UploadKeyPackageRequest, WelcomeMessage,
};
use futures::Stream;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use xmtp_common::RetryableError;

#[cfg(any(test, feature = "test-utils"))]
pub trait XmtpTestClient {
    type Builder: ApiBuilder;
    fn create_local() -> Self::Builder;
    fn create_local_d14n() -> Self::Builder;
    fn create_local_payer() -> Self::Builder;
    fn create_dev() -> Self::Builder;
}

pub type BoxedXmtpApi<Error> = Box<dyn trait_impls::BoxableXmtpApi<Error>>;
pub type ArcedXmtpApi<Error> = Arc<dyn trait_impls::BoxableXmtpApi<Error>>;

pub use trait_impls::*;

/// XMTP Api Super Trait
/// Implements all Trait Network APIs for convenience.
pub mod trait_impls {
    #[allow(unused)]
    #[cfg(any(test, feature = "test-utils"))]
    use super::XmtpTestClient;
    pub use inner::*;

    // native, release
    #[cfg(not(target_arch = "wasm32"))]
    mod inner {
        use crate::api_client::{XmtpIdentityClient, XmtpMlsClient};

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
    }

    // wasm32, release
    #[cfg(target_arch = "wasm32")]
    mod inner {

        pub trait BoxableXmtpApi<Err>
        where
            Self: XmtpMlsClient<Error = Err> + XmtpIdentityClient<Error = Err>,
        {
        }

        impl<T, Err> BoxableXmtpApi<Err> for T where
            T: XmtpMlsClient<Error = Err> + XmtpIdentityClient<Error = Err> + ?Sized
        {
        }

        use crate::api_client::{XmtpIdentityClient, XmtpMlsClient};
        pub trait XmtpApi
        where
            Self: XmtpMlsClient + XmtpIdentityClient,
        {
        }

        impl<T> XmtpApi for T where T: XmtpMlsClient + XmtpIdentityClient + ?Sized {}
    }
}

#[derive(Clone, Default, Debug)]
pub struct ApiStats {
    pub upload_key_package: Arc<EndpointStats>,
    pub fetch_key_package: Arc<EndpointStats>,
    pub send_group_messages: Arc<EndpointStats>,
    pub send_welcome_messages: Arc<EndpointStats>,
    pub query_group_messages: Arc<EndpointStats>,
    pub query_welcome_messages: Arc<EndpointStats>,
    pub subscribe_messages: Arc<EndpointStats>,
    pub subscribe_welcomes: Arc<EndpointStats>,
}

impl ApiStats {
    pub fn clear(&self) {
        self.upload_key_package.clear();
        self.fetch_key_package.clear();
        self.send_group_messages.clear();
        self.send_welcome_messages.clear();
        self.query_group_messages.clear();
        self.query_welcome_messages.clear();
        self.subscribe_messages.clear();
        self.subscribe_welcomes.clear();
    }
}

#[derive(Clone, Default, Debug)]
pub struct IdentityStats {
    pub publish_identity_update: Arc<EndpointStats>,
    pub get_identity_updates_v2: Arc<EndpointStats>,
    pub get_inbox_ids: Arc<EndpointStats>,
    pub verify_smart_contract_wallet_signature: Arc<EndpointStats>,
}

impl IdentityStats {
    pub fn clear(&self) {
        self.publish_identity_update.clear();
        self.get_identity_updates_v2.clear();
        self.get_inbox_ids.clear();
        self.verify_smart_contract_wallet_signature.clear();
    }
}

pub struct AggregateStats {
    pub mls: ApiStats,
    pub identity: IdentityStats,
}

impl std::fmt::Debug for AggregateStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "============ Api Stats ============")?;
        writeln!(f, "UploadKeyPackage        {}", self.mls.upload_key_package)?;
        writeln!(f, "FetchKeyPackage         {}", self.mls.fetch_key_package)?;
        writeln!(
            f,
            "SendGroupMessages       {}",
            self.mls.send_group_messages
        )?;
        writeln!(
            f,
            "SendWelcomeMessages     {}",
            self.mls.send_welcome_messages
        )?;
        writeln!(
            f,
            "QueryGroupMessages      {}",
            self.mls.query_group_messages
        )?;
        writeln!(
            f,
            "QueryWelcomeMessages    {}",
            self.mls.query_welcome_messages
        )?;
        writeln!(f, "SubscribeMessages       {}", self.mls.subscribe_messages)?;
        writeln!(f, "SubscribeWelcomes       {}", self.mls.subscribe_welcomes)?;
        writeln!(f, "============ Identity ============")?;
        writeln!(
            f,
            "PublishIdentityUpdate    {}",
            self.identity.publish_identity_update
        )?;
        writeln!(
            f,
            "GetIdentityUpdatesV2     {}",
            self.identity.get_identity_updates_v2
        )?;
        writeln!(f, "GetInboxIds             {}", self.identity.get_inbox_ids)?;
        writeln!(
            f,
            "VerifySCWSignatures     {}",
            self.identity.verify_smart_contract_wallet_signature
        )?;
        writeln!(f, "============ Stream ============")?;
        writeln!(
            f,
            "SubscribeMessages        {}",
            self.mls.subscribe_messages
        )?;
        writeln!(f, "SubscribeWelcomes       {}", self.mls.subscribe_welcomes)?;
        Ok(())
    }
}

#[derive(Default, Debug)]
pub struct EndpointStats {
    request_count: AtomicUsize,
}

impl std::fmt::Display for EndpointStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.request_count.load(Ordering::Relaxed))
    }
}

impl EndpointStats {
    pub fn count_request(&self) {
        self.request_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn get_count(&self) -> usize {
        self.request_count.load(Ordering::Relaxed)
    }
    pub fn clear(&self) {
        self.request_count.store(0, Ordering::Relaxed)
    }
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
        request: QueryGroupMessagesRequest,
    ) -> Result<QueryGroupMessagesResponse, Self::Error>;
    async fn query_welcome_messages(
        &self,
        request: QueryWelcomeMessagesRequest,
    ) -> Result<QueryWelcomeMessagesResponse, Self::Error>;
    fn stats(&self) -> ApiStats;
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<T> XmtpMlsClient for Box<T>
where
    T: XmtpMlsClient + Sync + ?Sized,
{
    type Error = <T as XmtpMlsClient>::Error;

    async fn upload_key_package(
        &self,
        request: UploadKeyPackageRequest,
    ) -> Result<(), Self::Error> {
        (**self).upload_key_package(request).await
    }

    async fn fetch_key_packages(
        &self,
        request: FetchKeyPackagesRequest,
    ) -> Result<FetchKeyPackagesResponse, Self::Error> {
        (**self).fetch_key_packages(request).await
    }

    async fn send_group_messages(
        &self,
        request: SendGroupMessagesRequest,
    ) -> Result<(), Self::Error> {
        (**self).send_group_messages(request).await
    }

    async fn send_welcome_messages(
        &self,
        request: SendWelcomeMessagesRequest,
    ) -> Result<(), Self::Error> {
        (**self).send_welcome_messages(request).await
    }

    async fn query_group_messages(
        &self,
        request: QueryGroupMessagesRequest,
    ) -> Result<QueryGroupMessagesResponse, Self::Error> {
        (**self).query_group_messages(request).await
    }

    async fn query_welcome_messages(
        &self,
        request: QueryWelcomeMessagesRequest,
    ) -> Result<QueryWelcomeMessagesResponse, Self::Error> {
        (**self).query_welcome_messages(request).await
    }

    fn stats(&self) -> ApiStats {
        (**self).stats()
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<T> XmtpMlsClient for Arc<T>
where
    T: XmtpMlsClient + Sync + ?Sized + Send,
{
    type Error = <T as XmtpMlsClient>::Error;

    async fn upload_key_package(
        &self,
        request: UploadKeyPackageRequest,
    ) -> Result<(), Self::Error> {
        (**self).upload_key_package(request).await
    }

    async fn fetch_key_packages(
        &self,
        request: FetchKeyPackagesRequest,
    ) -> Result<FetchKeyPackagesResponse, Self::Error> {
        (**self).fetch_key_packages(request).await
    }

    async fn send_group_messages(
        &self,
        request: SendGroupMessagesRequest,
    ) -> Result<(), Self::Error> {
        (**self).send_group_messages(request).await
    }

    async fn send_welcome_messages(
        &self,
        request: SendWelcomeMessagesRequest,
    ) -> Result<(), Self::Error> {
        (**self).send_welcome_messages(request).await
    }

    async fn query_group_messages(
        &self,
        request: QueryGroupMessagesRequest,
    ) -> Result<QueryGroupMessagesResponse, Self::Error> {
        (**self).query_group_messages(request).await
    }

    async fn query_welcome_messages(
        &self,
        request: QueryWelcomeMessagesRequest,
    ) -> Result<QueryWelcomeMessagesResponse, Self::Error> {
        (**self).query_welcome_messages(request).await
    }

    fn stats(&self) -> ApiStats {
        (**self).stats()
    }
}
#[cfg(not(target_arch = "wasm32"))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait XmtpMlsStreams {
    type GroupMessageStream: Stream<Item = Result<GroupMessage, Self::Error>> + Send;

    type WelcomeMessageStream: Stream<Item = Result<WelcomeMessage, Self::Error>> + Send;
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

#[cfg(target_arch = "wasm32")]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait XmtpMlsStreams {
    type GroupMessageStream: Stream<Item = Result<GroupMessage, Self::Error>>;
    type WelcomeMessageStream: Stream<Item = Result<WelcomeMessage, Self::Error>>;
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
impl<T> XmtpMlsStreams for Box<T>
where
    T: XmtpMlsStreams + Sync + ?Sized,
{
    type Error = <T as XmtpMlsStreams>::Error;

    type GroupMessageStream = <T as XmtpMlsStreams>::GroupMessageStream;

    type WelcomeMessageStream = <T as XmtpMlsStreams>::WelcomeMessageStream;

    async fn subscribe_group_messages(
        &self,
        request: SubscribeGroupMessagesRequest,
    ) -> Result<Self::GroupMessageStream, Self::Error> {
        (**self).subscribe_group_messages(request).await
    }

    async fn subscribe_welcome_messages(
        &self,
        request: SubscribeWelcomeMessagesRequest,
    ) -> Result<Self::WelcomeMessageStream, Self::Error> {
        (**self).subscribe_welcome_messages(request).await
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<T> XmtpMlsStreams for Arc<T>
where
    T: XmtpMlsStreams + Sync + ?Sized + Send,
{
    type Error = <T as XmtpMlsStreams>::Error;

    type GroupMessageStream = <T as XmtpMlsStreams>::GroupMessageStream;

    type WelcomeMessageStream = <T as XmtpMlsStreams>::WelcomeMessageStream;

    async fn subscribe_group_messages(
        &self,
        request: SubscribeGroupMessagesRequest,
    ) -> Result<Self::GroupMessageStream, Self::Error> {
        (**self).subscribe_group_messages(request).await
    }

    async fn subscribe_welcome_messages(
        &self,
        request: SubscribeWelcomeMessagesRequest,
    ) -> Result<Self::WelcomeMessageStream, Self::Error> {
        (**self).subscribe_welcome_messages(request).await
    }
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

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<T> XmtpIdentityClient for Box<T>
where
    T: XmtpIdentityClient + Send + Sync + ?Sized,
{
    type Error = <T as XmtpIdentityClient>::Error;

    async fn publish_identity_update(
        &self,
        request: PublishIdentityUpdateRequest,
    ) -> Result<PublishIdentityUpdateResponse, Self::Error> {
        (**self).publish_identity_update(request).await
    }

    async fn get_identity_updates_v2(
        &self,
        request: GetIdentityUpdatesV2Request,
    ) -> Result<GetIdentityUpdatesV2Response, Self::Error> {
        (**self).get_identity_updates_v2(request).await
    }

    async fn get_inbox_ids(
        &self,
        request: GetInboxIdsRequest,
    ) -> Result<GetInboxIdsResponse, Self::Error> {
        (**self).get_inbox_ids(request).await
    }

    async fn verify_smart_contract_wallet_signatures(
        &self,
        request: VerifySmartContractWalletSignaturesRequest,
    ) -> Result<VerifySmartContractWalletSignaturesResponse, Self::Error> {
        (**self)
            .verify_smart_contract_wallet_signatures(request)
            .await
    }
    fn identity_stats(&self) -> IdentityStats {
        (**self).identity_stats()
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<T> XmtpIdentityClient for Arc<T>
where
    T: XmtpIdentityClient + Send + Sync + ?Sized,
{
    type Error = <T as XmtpIdentityClient>::Error;

    async fn publish_identity_update(
        &self,
        request: PublishIdentityUpdateRequest,
    ) -> Result<PublishIdentityUpdateResponse, Self::Error> {
        (**self).publish_identity_update(request).await
    }

    async fn get_identity_updates_v2(
        &self,
        request: GetIdentityUpdatesV2Request,
    ) -> Result<GetIdentityUpdatesV2Response, Self::Error> {
        (**self).get_identity_updates_v2(request).await
    }

    async fn get_inbox_ids(
        &self,
        request: GetInboxIdsRequest,
    ) -> Result<GetInboxIdsResponse, Self::Error> {
        (**self).get_inbox_ids(request).await
    }

    async fn verify_smart_contract_wallet_signatures(
        &self,
        request: VerifySmartContractWalletSignaturesRequest,
    ) -> Result<VerifySmartContractWalletSignaturesResponse, Self::Error> {
        (**self)
            .verify_smart_contract_wallet_signatures(request)
            .await
    }

    fn identity_stats(&self) -> IdentityStats {
        (**self).identity_stats()
    }
}

pub trait ApiBuilder {
    type Output;
    type Error;

    /// set the libxmtp version (required)
    fn set_libxmtp_version(&mut self, version: String) -> Result<(), Self::Error>;

    /// set the sdk app version (required)
    fn set_app_version(&mut self, version: String) -> Result<(), Self::Error>;

    /// Set the libxmtp host (required)
    fn set_host(&mut self, host: String);

    /// Set the payer URL (optional)
    fn set_payer(&mut self, _host: String) {}

    /// indicate tls (default: false)
    fn set_tls(&mut self, tls: bool);

    /// Set  the rate limit per minute for this client
    fn rate_per_minute(&mut self, limit: u32);

    #[allow(async_fn_in_trait)]
    /// Build the api client
    async fn build(self) -> Result<Self::Output, Self::Error>;
}
