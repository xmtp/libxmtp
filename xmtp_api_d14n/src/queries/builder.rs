//! Generic Builder for the backend API

use derive_builder::UninitializedFieldError;
use std::sync::Arc;
use thiserror::Error;
use xmtp_api_grpc::error::{GrpcBuilderError, GrpcError};
use xmtp_common::{MaybeSend, MaybeSync};
use xmtp_id::scw_verifier::VerifierError;
use xmtp_proto::api::ApiClientError;
use xmtp_proto::types::AppVersion;

use crate::protocol::{CursorStore, FullXmtpApiArc, FullXmtpApiBox, NoCursorStore};
use crate::{
    ClientBundle, ClientBundleBuilder, ClientKind, D14nClient, MultiNodeClientBuilderError,
    ReadWriteClientBuilderError, V3Client,
};

mod impls;

/// Builder to access the backend XMTP API
/// Passing a gateway host implicitly enables decentralization.
#[derive(Clone, Default)]
pub struct MessageBackendBuilder {
    client_bundle: ClientBundleBuilder,
    cursor_store: Option<Arc<dyn CursorStore>>,
}

#[derive(Error, Debug)]
pub enum MessageBackendBuilderError {
    #[error("V3 Host is required")]
    MissingV3Host,
    #[error(transparent)]
    GrpcBuilder(#[from] GrpcBuilderError),
    #[error(transparent)]
    MultiNode(#[from] MultiNodeClientBuilderError),
    #[error(transparent)]
    Scw(#[from] VerifierError),
    #[error("failed to build stateful local client, cursor store not replaced, type {0}")]
    CursorStoreNotReplaced(&'static str),
    #[error("error while building read/write api client {0},")]
    UninitializedField(#[from] ReadWriteClientBuilderError),
    #[error(transparent)]
    Builder(#[from] UninitializedFieldError),
    #[error("client kind {0} is currently unsupported")]
    UnsupportedClient(ClientKind),
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
    /// Required
    pub fn v3_host<S: AsRef<str>>(&mut self, host: S) -> &mut Self {
        self.client_bundle.v3_host(host.as_ref());
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

    /// Indicate that the connection should use TLS
    pub fn is_secure(&mut self, is_secure: bool) -> &mut Self {
        self.client_bundle.is_secure(is_secure);
        self
    }

    pub fn cursor_store(&mut self, store: impl CursorStore + 'static) -> &mut Self {
        self.cursor_store = Some(Arc::new(store) as Arc<_>);
        self
    }

    pub fn from_bundle(
        &mut self,
        bundle: ClientBundle<GrpcError>,
    ) -> Result<FullXmtpApiArc<ApiClientError<GrpcError>>, MessageBackendBuilderError> {
        let Self { cursor_store, .. } = self.clone();
        let cursor_store = cursor_store.unwrap_or(Arc::new(NoCursorStore) as Arc<dyn CursorStore>);

        match bundle.kind() {
            ClientKind::D14n => Ok(D14nClient::new(bundle, cursor_store)?.arced()),
            ClientKind::V3 => Ok(V3Client::new(bundle, cursor_store).arced()),
            ClientKind::Hybrid => Err(MessageBackendBuilderError::UnsupportedClient(
                ClientKind::Hybrid,
            )),
        }
    }

    /// Build the client
    pub fn build(
        &mut self,
    ) -> Result<FullXmtpApiArc<ApiClientError<GrpcError>>, MessageBackendBuilderError> {
        let Self { client_bundle, .. } = self;
        let bundle = client_bundle.build()?;
        self.from_bundle(bundle)
    }
}
