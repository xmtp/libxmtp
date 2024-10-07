use std::{error::Error as StdError, fmt};

use futures::Stream;

pub use super::xmtp::message_api::v1::{
    BatchQueryRequest, BatchQueryResponse, Envelope, PagingInfo, PublishRequest, PublishResponse,
    QueryRequest, QueryResponse, SubscribeRequest,
};
use crate::xmtp::identity::api::v1::{
    GetIdentityUpdatesRequest as GetIdentityUpdatesV2Request,
    GetIdentityUpdatesResponse as GetIdentityUpdatesV2Response, GetInboxIdsRequest,
    GetInboxIdsResponse, PublishIdentityUpdateRequest, PublishIdentityUpdateResponse,
};
use crate::xmtp::mls::api::v1::{
    FetchKeyPackagesRequest, FetchKeyPackagesResponse, GroupMessage, QueryGroupMessagesRequest,
    QueryGroupMessagesResponse, QueryWelcomeMessagesRequest, QueryWelcomeMessagesResponse,
    SendGroupMessagesRequest, SendWelcomeMessagesRequest, SubscribeGroupMessagesRequest,
    SubscribeWelcomeMessagesRequest, UploadKeyPackageRequest, WelcomeMessage,
};

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
        f.write_str(match &self.kind {
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
        })?;
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

#[allow(async_fn_in_trait)]
pub trait MutableApiSubscription: Stream<Item = Result<Envelope, Error>> + Send {
    async fn update(&mut self, req: SubscribeRequest) -> Result<(), Error>;
    fn close(&self);
}

pub trait ClientWithMetadata {
    fn set_libxmtp_version(&mut self, version: String) -> Result<(), Error>;
    fn set_app_version(&mut self, version: String) -> Result<(), Error>;
}

/// Global Marker trait for WebAssembly
#[cfg(target_arch = "wasm32")]
pub trait Wasm {}
#[cfg(target_arch = "wasm32")]
impl<T> Wasm for T {}

// Wasm futures don't have `Send` or `Sync` bounds.
#[allow(async_fn_in_trait)]
#[cfg_attr(not(target_arch = "wasm32"), trait_variant::make(XmtpApiClient: Send))]
#[cfg_attr(target_arch = "wasm32", trait_variant::make(XmtpApiClient: Wasm))]
pub trait LocalXmtpApiClient {
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

// Wasm futures don't have `Send` or `Sync` bounds.
#[allow(async_fn_in_trait)]
#[cfg_attr(not(target_arch = "wasm32"), trait_variant::make(XmtpMlsClient: Send))]
#[cfg_attr(target_arch = "wasm32", trait_variant::make(XmtpMlsClient: Wasm))]
pub trait LocalXmtpMlsClient {
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

#[allow(async_fn_in_trait)]
#[cfg_attr(target_arch = "wasm32", trait_variant::make(XmtpMlsStreams: Wasm))]
pub trait LocalXmtpMlsStreams {
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

// we manually make a Local+Non-Local trait variant here b/c the
// macro breaks with GATs
#[allow(async_fn_in_trait)]
#[cfg(not(target_arch = "wasm32"))]
pub trait XmtpMlsStreams: Send {
    type GroupMessageStream<'a>: Stream<Item = Result<GroupMessage, Error>> + Send + 'a
    where
        Self: 'a;

    type WelcomeMessageStream<'a>: Stream<Item = Result<WelcomeMessage, Error>> + Send + 'a
    where
        Self: 'a;

    fn subscribe_group_messages(
        &self,
        request: SubscribeGroupMessagesRequest,
    ) -> impl futures::Future<Output = Result<Self::GroupMessageStream<'_>, Error>> + Send;

    fn subscribe_welcome_messages(
        &self,
        request: SubscribeWelcomeMessagesRequest,
    ) -> impl futures::Future<Output = Result<Self::WelcomeMessageStream<'_>, Error>> + Send;
}

#[allow(async_fn_in_trait)]
#[cfg_attr(not(target_arch = "wasm32"), trait_variant::make(XmtpIdentityClient: Send))]
#[cfg_attr(target_arch = "wasm32", trait_variant::make(XmtpIdentityClient: Wasm))]
pub trait LocalXmtpIdentityClient {
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
}
