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

#[cfg(any(test, feature = "test-utils"))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait XmtpTestClient {
    async fn create_local() -> Self;
    async fn create_dev() -> Self;
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

pub trait XmtpApiSubscription {
    fn is_closed(&self) -> bool;
    fn get_messages(&self) -> Vec<Envelope>;
    fn close_stream(&mut self);
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait MutableApiSubscription: Stream<Item = Result<Envelope, Self::Error>> + Send {
    type Error;
    async fn update(&mut self, req: SubscribeRequest) -> Result<(), Self::Error>;
    fn close(&self);
}

// Wasm futures don't have `Send` or `Sync` bounds.
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait XmtpApiClient {
    type Subscription: XmtpApiSubscription;
    type MutableSubscription: MutableApiSubscription;
    type Error;

    async fn publish(
        &self,
        token: String,
        request: PublishRequest,
    ) -> Result<PublishResponse, Self::Error>;

    async fn subscribe(&self, request: SubscribeRequest)
        -> Result<Self::Subscription, Self::Error>;

    async fn subscribe2(
        &self,
        request: SubscribeRequest,
    ) -> Result<Self::MutableSubscription, Self::Error>;

    async fn query(&self, request: QueryRequest) -> Result<QueryResponse, Self::Error>;

    async fn batch_query(
        &self,
        request: BatchQueryRequest,
    ) -> Result<BatchQueryResponse, Self::Error>;
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<T> XmtpApiClient for Box<T>
where
    T: XmtpApiClient + Sync + ?Sized,
{
    type Subscription = <T as XmtpApiClient>::Subscription;
    type MutableSubscription = <T as XmtpApiClient>::MutableSubscription;
    type Error = <T as XmtpApiClient>::Error;

    async fn publish(
        &self,
        token: String,
        request: PublishRequest,
    ) -> Result<PublishResponse, Self::Error> {
        (**self).publish(token, request).await
    }

    async fn subscribe(
        &self,
        request: SubscribeRequest,
    ) -> Result<Self::Subscription, Self::Error> {
        (**self).subscribe(request).await
    }

    async fn subscribe2(
        &self,
        request: SubscribeRequest,
    ) -> Result<Self::MutableSubscription, Self::Error> {
        (**self).subscribe2(request).await
    }

    async fn query(&self, request: QueryRequest) -> Result<QueryResponse, Self::Error> {
        (**self).query(request).await
    }

    async fn batch_query(
        &self,
        request: BatchQueryRequest,
    ) -> Result<BatchQueryResponse, Self::Error> {
        (**self).batch_query(request).await
    }
}

#[derive(Clone, Default)]
pub struct ApiStats {
    pub upload_key_package: Arc<EndpointStats>,
    pub fetch_key_package: Arc<EndpointStats>,
    pub send_group_messages: Arc<EndpointStats>,
    pub send_welcome_messages: Arc<EndpointStats>,
    pub query_group_messages: Arc<EndpointStats>,
    pub query_welcome_messages: Arc<EndpointStats>,
}

#[derive(Default)]
pub struct EndpointStats {
    request_count: AtomicUsize,
}

impl EndpointStats {
    pub fn count_request(&self) {
        self.request_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn request_count(&self) -> usize {
        self.request_count.load(Ordering::Relaxed)
    }
}

// Wasm futures don't have `Send` or `Sync` bounds.
#[allow(async_fn_in_trait)]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait XmtpMlsClient {
    type Error: crate::XmtpApiError + 'static;
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
    fn stats(&self) -> &ApiStats;
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

    fn stats(&self) -> &ApiStats {
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

    fn stats(&self) -> &ApiStats {
        (**self).stats()
    }
}
#[cfg(not(target_arch = "wasm32"))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait XmtpMlsStreams {
    type GroupMessageStream<'a>: Stream<Item = Result<GroupMessage, Self::Error>> + Send + 'a
    where
        Self: 'a;

    type WelcomeMessageStream<'a>: Stream<Item = Result<WelcomeMessage, Self::Error>> + Send + 'a
    where
        Self: 'a;
    type Error: crate::XmtpApiError + 'static;

    async fn subscribe_group_messages(
        &self,
        request: SubscribeGroupMessagesRequest,
    ) -> Result<Self::GroupMessageStream<'_>, Self::Error>;
    async fn subscribe_welcome_messages(
        &self,
        request: SubscribeWelcomeMessagesRequest,
    ) -> Result<Self::WelcomeMessageStream<'_>, Self::Error>;
}

#[cfg(target_arch = "wasm32")]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait XmtpMlsStreams {
    type GroupMessageStream<'a>: Stream<Item = Result<GroupMessage, Self::Error>> + 'a
    where
        Self: 'a;

    type WelcomeMessageStream<'a>: Stream<Item = Result<WelcomeMessage, Self::Error>> + 'a
    where
        Self: 'a;
    type Error: crate::XmtpApiError + 'static;

    async fn subscribe_group_messages(
        &self,
        request: SubscribeGroupMessagesRequest,
    ) -> Result<Self::GroupMessageStream<'_>, Self::Error>;
    async fn subscribe_welcome_messages(
        &self,
        request: SubscribeWelcomeMessagesRequest,
    ) -> Result<Self::WelcomeMessageStream<'_>, Self::Error>;
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<T> XmtpMlsStreams for Box<T>
where
    T: XmtpMlsStreams + Sync + ?Sized,
{
    type Error = <T as XmtpMlsStreams>::Error;

    type GroupMessageStream<'a>
        = <T as XmtpMlsStreams>::GroupMessageStream<'a>
    where
        Self: 'a;

    type WelcomeMessageStream<'a>
        = <T as XmtpMlsStreams>::WelcomeMessageStream<'a>
    where
        Self: 'a;

    async fn subscribe_group_messages(
        &self,
        request: SubscribeGroupMessagesRequest,
    ) -> Result<Self::GroupMessageStream<'_>, Self::Error> {
        (**self).subscribe_group_messages(request).await
    }

    async fn subscribe_welcome_messages(
        &self,
        request: SubscribeWelcomeMessagesRequest,
    ) -> Result<Self::WelcomeMessageStream<'_>, Self::Error> {
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

    type GroupMessageStream<'a>
        = <T as XmtpMlsStreams>::GroupMessageStream<'a>
    where
        Self: 'a;

    type WelcomeMessageStream<'a>
        = <T as XmtpMlsStreams>::WelcomeMessageStream<'a>
    where
        Self: 'a;

    async fn subscribe_group_messages(
        &self,
        request: SubscribeGroupMessagesRequest,
    ) -> Result<Self::GroupMessageStream<'_>, Self::Error> {
        (**self).subscribe_group_messages(request).await
    }

    async fn subscribe_welcome_messages(
        &self,
        request: SubscribeWelcomeMessagesRequest,
    ) -> Result<Self::WelcomeMessageStream<'_>, Self::Error> {
        (**self).subscribe_welcome_messages(request).await
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait XmtpIdentityClient {
    type Error: crate::XmtpApiError + 'static;
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

    #[allow(async_fn_in_trait)]
    /// Build the api client
    async fn build(self) -> Result<Self::Output, Self::Error>;
}
