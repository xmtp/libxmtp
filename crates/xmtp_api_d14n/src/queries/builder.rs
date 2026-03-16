//! Generic Builder for the backend API

use derive_builder::UninitializedFieldError;
use std::sync::Arc;
use thiserror::Error;
use xmtp_api_grpc::error::GrpcBuilderError;
use xmtp_common::{ErrorCode, MaybeSend, MaybeSync};
use xmtp_id::scw_verifier::VerifierError;
use xmtp_proto::types::AppVersion;

use crate::definitions::XmtpApiClient;
use crate::protocol::{CursorStore, FullXmtpApiArc, FullXmtpApiBox, NoCursorStore};
use crate::{
    AuthCallback, AuthHandle, ClientBundle, ClientBundleBuilder, D14nClient, MigrationClient,
    MultiNodeClientBuilderError, ReadWriteClientBuilderError, ReadonlyClientBuilderError, V3Client,
};

mod impls;

/// Builder to access the backend XMTP API
/// Passing a gateway host implicitly enables decentralization.
#[derive(Clone, Default)]
pub struct MessageBackendBuilder {
    client_bundle: ClientBundleBuilder,
    cursor_store: Option<Arc<dyn CursorStore>>,
}

#[derive(Error, Debug, ErrorCode)]
pub enum MessageBackendBuilderError {
    /// Missing V3 host.
    ///
    /// V3 host was not set on the builder. Not retryable.
    #[error("V3 Host is Required")]
    MissingV3Host,
    /// gRPC builder error.
    ///
    /// gRPC client builder failed. Not retryable.
    #[error(transparent)]
    GrpcBuilder(#[from] GrpcBuilderError),
    /// Multi-node error.
    ///
    /// Multi-node client builder failed. Not retryable.
    #[error(transparent)]
    MultiNode(#[from] MultiNodeClientBuilderError),
    /// SCW verifier error.
    ///
    /// Smart contract wallet verifier error. Not retryable.
    #[error(transparent)]
    Scw(#[from] VerifierError),
    /// Cursor store not replaced.
    ///
    /// Stateful client cursor store not set. Not retryable.
    #[error("failed to build stateful local client, cursor store not replaced, type {0}")]
    CursorStoreNotReplaced(&'static str),
    /// Read/write client builder error.
    ///
    /// Read/write client builder failed. Not retryable.
    #[error("error while building read/write api client {0},")]
    UninitializedField(#[from] ReadWriteClientBuilderError),
    /// Readonly builder error.
    ///
    /// Readonly client builder failed. Not retryable.
    #[error(transparent)]
    ReadonlyBuilder(#[from] ReadonlyClientBuilderError),
    /// Builder error.
    ///
    /// Uninitialized field in builder. Not retryable.
    #[error(transparent)]
    Builder(#[from] UninitializedFieldError),
    /// Missing XMTP Gateway host.
    ///
    /// XMTP Gateway host was not set on the builder. Not retryable.
    #[error("XMTP Gateway host is required")]
    MissingGatewayHost,

    /// Invalid host URL given
    ///
    /// Url is not valid. Not retryable.
    #[error("URL {url} given is invalid because {source}")]
    InvalidUrl {
        url: String,
        source: url::ParseError,
    },
}

impl MessageBackendBuilderError {
    pub fn invalid_url(e: url::ParseError, url: String) -> Self {
        MessageBackendBuilderError::InvalidUrl { url, source: e }
    }
}

/// Indicates this api implementation can be type-erased
/// and coerced into a [`Box`] or [`Arc`]
pub trait ToDynApi: MaybeSend + MaybeSync {
    type Error: MaybeSend + MaybeSync;
    fn boxed(self) -> FullXmtpApiBox<Self::Error>;
    fn arced(self) -> FullXmtpApiArc<Self::Error>;
}

impl MessageBackendBuilder {
    /// An optional field which allows inbox apps to specify their version
    pub fn app_version(&mut self, version: impl Into<AppVersion>) -> &mut Self {
        self.client_bundle.app_version(version);
        self
    }

    /// Specify the node host
    /// for d14n this is the replication node
    /// for v3 this is xmtp-node-go
    ///
    /// Required for V3 mode; optional when gateway_host is provided (D14n mode).
    pub fn v3_host<S: AsRef<str>>(&mut self, host: S) -> &mut Self {
        self.client_bundle.v3_host(host.as_ref());
        self
    }

    /// Specify the node host as an Option<T>
    /// for d14n this is the replication node
    /// for v3 this is xmtp-node-go
    ///
    /// Required for V3 mode; optional when gateway_host is provided (D14n mode).
    pub fn maybe_v3_host<S: Into<String>>(&mut self, host: Option<S>) -> &mut Self {
        self.client_bundle.maybe_v3_host(host);
        self
    }

    /// Specify the gateway host
    /// the gateway is a d14n-specific host
    /// specifying this fields implicitly enables decentralization
    ///
    /// Optional
    pub fn gateway_host<S: AsRef<str>>(&mut self, host: S) -> &mut Self {
        self.client_bundle.gateway_host(host.as_ref());
        self
    }

    /// Specify the gateway host as an Option<T>
    /// the gateway is a d14n-specific host
    /// specifying this fields implicitly enables decentralization
    ///
    /// Optional
    pub fn maybe_gateway_host<S: Into<String>>(&mut self, gateway_host: Option<S>) -> &mut Self {
        self.client_bundle.maybe_gateway_host(gateway_host);
        self
    }

    pub fn cursor_store(&mut self, store: impl CursorStore + 'static) -> &mut Self {
        self.cursor_store = Some(Arc::new(store) as Arc<_>);
        self
    }

    pub fn readonly(&mut self, readonly: bool) -> &mut Self {
        self.client_bundle.readonly(readonly);
        self
    }

    pub fn from_bundle(
        &mut self,
        bundle: ClientBundle,
    ) -> Result<XmtpApiClient, MessageBackendBuilderError> {
        let cursor_store = self
            .cursor_store
            .clone()
            .unwrap_or(Arc::new(NoCursorStore) as Arc<dyn CursorStore>);

        match bundle {
            ClientBundle::D14n(c) => Ok(D14nClient::new(c, cursor_store)?.arced()),
            ClientBundle::V3(c) => Ok(V3Client::new(c, cursor_store).arced()),
            ClientBundle::Migration { v3, xmtpd } => {
                Ok(MigrationClient::new(v3, xmtpd, cursor_store)?.arced())
            }
        }
    }

    pub fn maybe_auth_callback(&mut self, callback: Option<Arc<dyn AuthCallback>>) -> &mut Self {
        self.client_bundle.maybe_auth_callback(callback);
        self
    }

    pub fn maybe_auth_handle(&mut self, handle: Option<AuthHandle>) -> &mut Self {
        self.client_bundle.maybe_auth_handle(handle);
        self
    }

    /// Build the default Migration client
    /// Errors if either of V3 Host or Gateway host are missing
    pub fn build(&mut self) -> Result<XmtpApiClient, MessageBackendBuilderError> {
        let Self { client_bundle, .. } = self;
        let bundle = client_bundle.build()?;
        self.from_bundle(bundle)
    }

    /// Build a V3 Only Client
    /// Errors if the V3 Host is Missing
    pub fn build_v3(&mut self) -> Result<XmtpApiClient, MessageBackendBuilderError> {
        let Self { client_bundle, .. } = self;
        let bundle = client_bundle.build_v3()?;
        self.from_bundle(bundle)
    }

    /// Builds a d14n-only client
    /// Errors if the Gateway Host is missing
    pub fn build_d14n(&mut self) -> Result<XmtpApiClient, MessageBackendBuilderError> {
        let Self { client_bundle, .. } = self;
        let bundle = client_bundle.build_d14n()?;
        self.from_bundle(bundle)
    }

    /// If a gateway host is present, builds d14n-only
    /// otherwise builds a v3 client
    /// Errors if V3 Host is missing
    pub fn build_optional_d14n(&mut self) -> Result<XmtpApiClient, MessageBackendBuilderError> {
        let Self { client_bundle, .. } = self;
        let bundle = client_bundle.build_optional_d14n()?;
        self.from_bundle(bundle)
    }
}
