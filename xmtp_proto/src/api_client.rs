use async_trait::async_trait;
use std::{error::Error as StdError, fmt};

use crate::xmtp::message_api::v3::{
    ConsumeKeyPackagesRequest, ConsumeKeyPackagesResponse, GetIdentityUpdatesRequest,
    GetIdentityUpdatesResponse, PublishToGroupRequest, PublishWelcomesRequest,
    RegisterInstallationRequest, RegisterInstallationResponse, UploadKeyPackagesRequest,
};

pub use super::xmtp::message_api::v1::{
    BatchQueryRequest, BatchQueryResponse, Envelope, PagingInfo, PublishRequest, PublishResponse,
    QueryRequest, QueryResponse, SubscribeRequest,
};

#[derive(Debug)]
pub enum ErrorKind {
    SetupError,
    PublishError,
    QueryError,
    SubscribeError,
    BatchQueryError,
    MlsError,
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
            ErrorKind::SetupError => "setup error",
            ErrorKind::PublishError => "publish error",
            ErrorKind::QueryError => "query error",
            ErrorKind::SubscribeError => "subscribe error",
            ErrorKind::BatchQueryError => "batch query error",
            ErrorKind::MlsError => "mls error",
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

// Wasm futures don't have `Send` or `Sync` bounds.
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait XmtpApiClient {
    type Subscription: XmtpApiSubscription;

    fn set_app_version(&mut self, version: String);

    async fn publish(
        &self,
        token: String,
        request: PublishRequest,
    ) -> Result<PublishResponse, Error>;

    async fn subscribe(&self, request: SubscribeRequest) -> Result<Self::Subscription, Error>;

    async fn query(&self, request: QueryRequest) -> Result<QueryResponse, Error>;

    async fn batch_query(&self, request: BatchQueryRequest) -> Result<BatchQueryResponse, Error>;
}

// Wasm futures don't have `Send` or `Sync` bounds.
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait XmtpMlsClient {
    async fn register_installation(
        &self,
        request: RegisterInstallationRequest,
    ) -> Result<RegisterInstallationResponse, Error>;
    async fn upload_key_packages(&self, request: UploadKeyPackagesRequest) -> Result<(), Error>;
    async fn consume_key_packages(
        &self,
        request: ConsumeKeyPackagesRequest,
    ) -> Result<ConsumeKeyPackagesResponse, Error>;
    async fn publish_to_group(&self, request: PublishToGroupRequest) -> Result<(), Error>;
    async fn publish_welcomes(&self, request: PublishWelcomesRequest) -> Result<(), Error>;
    async fn get_identity_updates(
        &self,
        request: GetIdentityUpdatesRequest,
    ) -> Result<GetIdentityUpdatesResponse, Error>;
}
