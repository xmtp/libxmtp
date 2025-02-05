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

#[cfg(any(test, feature = "test-utils"))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait XmtpTestClient {
    async fn create_local() -> Self;
    async fn create_dev() -> Self;
}

/// XMTP Api Super Trait
/// Implements all Trait Network APIs for convenience.
pub mod trait_impls {
    #[allow(unused)]
    #[cfg(any(test, feature = "test-utils"))]
    use super::XmtpTestClient;
    pub use inner::*;

    // native, release
    #[cfg(all(not(feature = "test-utils"), not(target_arch = "wasm32")))]
    mod inner {
        use crate::api_client::{ClientWithMetadata, XmtpIdentityClient, XmtpMlsClient};

        pub trait XmtpApi
        where
            Self: XmtpMlsClient + XmtpIdentityClient + ClientWithMetadata + Send + Sync,
        {
        }
        impl<T> XmtpApi for T where
            T: XmtpMlsClient + XmtpIdentityClient + ClientWithMetadata + Send + Sync + ?Sized
        {
        }
    }

    // wasm32, release
    #[cfg(all(not(feature = "test-utils"), target_arch = "wasm32"))]
    mod inner {

        use crate::api_client::{ClientWithMetadata, XmtpIdentityClient, XmtpMlsClient};
        pub trait XmtpApi
        where
            Self: XmtpMlsClient + XmtpIdentityClient + ClientWithMetadata,
        {
        }

        impl<T> XmtpApi for T where T: XmtpMlsClient + XmtpIdentityClient + ClientWithMetadata + ?Sized {}
    }

    // test, native
    #[cfg(all(feature = "test-utils", not(target_arch = "wasm32")))]
    mod inner {
        use crate::api_client::{ClientWithMetadata, XmtpIdentityClient, XmtpMlsClient};

        pub trait XmtpApi
        where
            Self: XmtpMlsClient + XmtpIdentityClient + ClientWithMetadata + Send + Sync,
        {
        }
        impl<T> XmtpApi for T where
            T: XmtpMlsClient + XmtpIdentityClient + ClientWithMetadata + Send + Sync + ?Sized
        {
        }
    }

    // test, wasm32
    #[cfg(all(feature = "test-utils", target_arch = "wasm32"))]
    mod inner {
        use crate::api_client::{ClientWithMetadata, XmtpIdentityClient, XmtpMlsClient};

        pub trait XmtpApi
        where
            Self: XmtpMlsClient + XmtpIdentityClient + ClientWithMetadata,
        {
        }

        impl<T> XmtpApi for T where
            T: XmtpMlsClient + XmtpIdentityClient + ClientWithMetadata + Send + Sync + ?Sized
        {
        }
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

pub trait ClientWithMetadata {
    type Error;
    fn set_libxmtp_version(&mut self, version: String) -> Result<(), Self::Error>;
    fn set_app_version(&mut self, version: String) -> Result<(), Self::Error>;
}

impl<T> ClientWithMetadata for Box<T>
where
    T: ClientWithMetadata + ?Sized,
{
    type Error = <T as ClientWithMetadata>::Error;

    fn set_libxmtp_version(&mut self, version: String) -> Result<(), Self::Error> {
        (**self).set_libxmtp_version(version)
    }

    fn set_app_version(&mut self, version: String) -> Result<(), Self::Error> {
        (**self).set_app_version(version)
    }
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
