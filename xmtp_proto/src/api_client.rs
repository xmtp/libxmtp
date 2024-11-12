use std::{error::Error as StdError, fmt};

use futures::Stream;
use crate::api_client::ErrorKind::DecodingError;
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

#[derive(Debug)]
pub enum ErrorKind {
    SetupCreateChannelError,
    SetupTLSConfigError,
    SetupConnectionError,
    PublishError,
    QueryError,
    SubscribeError,
    BatchQueryError,
    MlsError,
    IdentityError,
    SubscriptionUpdateError,
    MetadataError,
    MissingPayloadError,
    DecodingError(hex::FromHexError),
}

type ErrorSource = Box<dyn StdError + Send + Sync + 'static>;

pub struct Error {
    kind: ErrorKind,
    source: Option<ErrorSource>,
}

impl Error {
    pub fn new(kind: ErrorKind) -> Self {
        Self { kind, source: None }
    }

    pub fn with(mut self, source: impl Into<ErrorSource>) -> Self {
        self.source = Some(source.into());
        self
    }
}

impl From<hex::FromHexError> for Error {
    fn from(err: hex::FromHexError) -> Self {
        Error::new(DecodingError(err))
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut f = f.debug_tuple("xmtp::error::Error");

        f.field(&self.kind);

        if let Some(source) = &self.source {
            f.field(source);
        }

        f.finish()
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match &self.kind {
            ErrorKind::SetupCreateChannelError => "failed to create channel",
            ErrorKind::SetupTLSConfigError => "tls configuration failed",
            ErrorKind::SetupConnectionError => "connection failed",
            ErrorKind::PublishError => "publish error",
            ErrorKind::QueryError => "query error",
            ErrorKind::SubscribeError => "subscribe error",
            ErrorKind::BatchQueryError => "batch query error",
            ErrorKind::IdentityError => "identity error",
            ErrorKind::MlsError => "mls error",
            ErrorKind::SubscriptionUpdateError => "subscription update error",
            ErrorKind::MetadataError => "metadata error",
            ErrorKind::MissingPayloadError => "missing payload error",
            DecodingError(_) => "could not convert payload from hex"
        };
        f.write_str(s)?;
        if self.source().is_some() {
            f.write_str(": ")?;
            f.write_str(&self.source().unwrap().to_string())?;
        }
        Ok(())
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        self.source
            .as_ref()
            .map(|source| &**source as &(dyn StdError + 'static))
    }
}

pub trait XmtpApiSubscription {
    fn is_closed(&self) -> bool;
    fn get_messages(&self) -> Vec<Envelope>;
    fn close_stream(&mut self);
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait MutableApiSubscription: Stream<Item = Result<Envelope, Error>> + Send {
    async fn update(&mut self, req: SubscribeRequest) -> Result<(), Error>;
    fn close(&self);
}

pub trait ClientWithMetadata {
    fn set_libxmtp_version(&mut self, version: String) -> Result<(), Error>;
    fn set_app_version(&mut self, version: String) -> Result<(), Error>;
}

impl<T> ClientWithMetadata for Box<T>
where
    T: ClientWithMetadata + ?Sized,
{
    fn set_libxmtp_version(&mut self, version: String) -> Result<(), Error> {
        (**self).set_libxmtp_version(version)
    }

    fn set_app_version(&mut self, version: String) -> Result<(), Error> {
        (**self).set_app_version(version)
    }
}

/// Global Marker trait for WebAssembly
#[cfg(target_arch = "wasm32")]
pub trait Wasm {}
#[cfg(target_arch = "wasm32")]
impl<T> Wasm for T {}

// Wasm futures don't have `Send` or `Sync` bounds.
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait XmtpApiClient {
    type Subscription: XmtpApiSubscription;
    type MutableSubscription: MutableApiSubscription;

    async fn publish(
        &self,
        token: String,
        request: PublishRequest,
    ) -> Result<PublishResponse, Error>;

    async fn subscribe(&self, request: SubscribeRequest) -> Result<Self::Subscription, Error>;

    async fn subscribe2(
        &self,
        request: SubscribeRequest,
    ) -> Result<Self::MutableSubscription, Error>;

    async fn query(&self, request: QueryRequest) -> Result<QueryResponse, Error>;

    async fn batch_query(&self, request: BatchQueryRequest) -> Result<BatchQueryResponse, Error>;
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<T> XmtpApiClient for Box<T>
where
    T: XmtpApiClient + Sync + ?Sized,
{
    type Subscription = <T as XmtpApiClient>::Subscription;

    type MutableSubscription = <T as XmtpApiClient>::MutableSubscription;

    async fn publish(
        &self,
        token: String,
        request: PublishRequest,
    ) -> Result<PublishResponse, Error> {
        (**self).publish(token, request).await
    }

    async fn subscribe(&self, request: SubscribeRequest) -> Result<Self::Subscription, Error> {
        (**self).subscribe(request).await
    }

    async fn subscribe2(
        &self,
        request: SubscribeRequest,
    ) -> Result<Self::MutableSubscription, Error> {
        (**self).subscribe2(request).await
    }

    async fn query(&self, request: QueryRequest) -> Result<QueryResponse, Error> {
        (**self).query(request).await
    }

    async fn batch_query(&self, request: BatchQueryRequest) -> Result<BatchQueryResponse, Error> {
        (**self).batch_query(request).await
    }
}

// Wasm futures don't have `Send` or `Sync` bounds.
#[allow(async_fn_in_trait)]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait XmtpMlsClient {
    async fn upload_key_package(&self, request: UploadKeyPackageRequest) -> Result<(), Error>;
    async fn fetch_key_packages(
        &self,
        request: FetchKeyPackagesRequest,
    ) -> Result<FetchKeyPackagesResponse, Error>;
    async fn send_group_messages(&self, request: SendGroupMessagesRequest) -> Result<(), Error>;
    async fn send_welcome_messages(&self, request: SendWelcomeMessagesRequest)
        -> Result<(), Error>;
    async fn query_group_messages(
        &self,
        request: QueryGroupMessagesRequest,
    ) -> Result<QueryGroupMessagesResponse, Error>;
    async fn query_welcome_messages(
        &self,
        request: QueryWelcomeMessagesRequest,
    ) -> Result<QueryWelcomeMessagesResponse, Error>;
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<T> XmtpMlsClient for Box<T>
where
    T: XmtpMlsClient + Sync + ?Sized,
{
    async fn upload_key_package(&self, request: UploadKeyPackageRequest) -> Result<(), Error> {
        (**self).upload_key_package(request).await
    }

    async fn fetch_key_packages(
        &self,
        request: FetchKeyPackagesRequest,
    ) -> Result<FetchKeyPackagesResponse, Error> {
        (**self).fetch_key_packages(request).await
    }

    async fn send_group_messages(&self, request: SendGroupMessagesRequest) -> Result<(), Error> {
        (**self).send_group_messages(request).await
    }

    async fn send_welcome_messages(
        &self,
        request: SendWelcomeMessagesRequest,
    ) -> Result<(), Error> {
        (**self).send_welcome_messages(request).await
    }

    async fn query_group_messages(
        &self,
        request: QueryGroupMessagesRequest,
    ) -> Result<QueryGroupMessagesResponse, Error> {
        (**self).query_group_messages(request).await
    }

    async fn query_welcome_messages(
        &self,
        request: QueryWelcomeMessagesRequest,
    ) -> Result<QueryWelcomeMessagesResponse, Error> {
        (**self).query_welcome_messages(request).await
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait XmtpMlsStreams {
    type GroupMessageStream<'a>: Stream<Item = Result<GroupMessage, Error>> + Send + 'a
    where
        Self: 'a;

    type WelcomeMessageStream<'a>: Stream<Item = Result<WelcomeMessage, Error>> + Send + 'a
    where
        Self: 'a;

    async fn subscribe_group_messages(
        &self,
        request: SubscribeGroupMessagesRequest,
    ) -> Result<Self::GroupMessageStream<'_>, Error>;
    async fn subscribe_welcome_messages(
        &self,
        request: SubscribeWelcomeMessagesRequest,
    ) -> Result<Self::WelcomeMessageStream<'_>, Error>;
}

#[cfg(target_arch = "wasm32")]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait XmtpMlsStreams {
    type GroupMessageStream<'a>: Stream<Item = Result<GroupMessage, Error>> + 'a
    where
        Self: 'a;

    type WelcomeMessageStream<'a>: Stream<Item = Result<WelcomeMessage, Error>> + 'a
    where
        Self: 'a;

    async fn subscribe_group_messages(
        &self,
        request: SubscribeGroupMessagesRequest,
    ) -> Result<Self::GroupMessageStream<'_>, Error>;
    async fn subscribe_welcome_messages(
        &self,
        request: SubscribeWelcomeMessagesRequest,
    ) -> Result<Self::WelcomeMessageStream<'_>, Error>;
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<T> XmtpMlsStreams for Box<T>
where
    T: XmtpMlsStreams + Sync + ?Sized,
{
    type GroupMessageStream<'a> = <T as XmtpMlsStreams>::GroupMessageStream<'a>
    where
        Self: 'a;

    type WelcomeMessageStream<'a> = <T as XmtpMlsStreams>::WelcomeMessageStream<'a>
    where
        Self: 'a;

    async fn subscribe_group_messages(
        &self,
        request: SubscribeGroupMessagesRequest,
    ) -> Result<Self::GroupMessageStream<'_>, Error> {
        (**self).subscribe_group_messages(request).await
    }

    async fn subscribe_welcome_messages(
        &self,
        request: SubscribeWelcomeMessagesRequest,
    ) -> Result<Self::WelcomeMessageStream<'_>, Error> {
        (**self).subscribe_welcome_messages(request).await
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait XmtpIdentityClient {
    async fn publish_identity_update(
        &self,
        request: PublishIdentityUpdateRequest,
    ) -> Result<PublishIdentityUpdateResponse, Error>;

    async fn get_identity_updates_v2(
        &self,
        request: GetIdentityUpdatesV2Request,
    ) -> Result<GetIdentityUpdatesV2Response, Error>;

    async fn get_inbox_ids(
        &self,
        request: GetInboxIdsRequest,
    ) -> Result<GetInboxIdsResponse, Error>;

    async fn verify_smart_contract_wallet_signatures(
        &self,
        request: VerifySmartContractWalletSignaturesRequest,
    ) -> Result<VerifySmartContractWalletSignaturesResponse, Error>;
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<T> XmtpIdentityClient for Box<T>
where
    T: XmtpIdentityClient + Send + Sync + ?Sized,
{
    async fn publish_identity_update(
        &self,
        request: PublishIdentityUpdateRequest,
    ) -> Result<PublishIdentityUpdateResponse, Error> {
        (**self).publish_identity_update(request).await
    }

    async fn get_identity_updates_v2(
        &self,
        request: GetIdentityUpdatesV2Request,
    ) -> Result<GetIdentityUpdatesV2Response, Error> {
        (**self).get_identity_updates_v2(request).await
    }

    async fn get_inbox_ids(
        &self,
        request: GetInboxIdsRequest,
    ) -> Result<GetInboxIdsResponse, Error> {
        (**self).get_inbox_ids(request).await
    }

    async fn verify_smart_contract_wallet_signatures(
        &self,
        request: VerifySmartContractWalletSignaturesRequest,
    ) -> Result<VerifySmartContractWalletSignaturesResponse, Error> {
        (**self)
            .verify_smart_contract_wallet_signatures(request)
            .await
    }
}
