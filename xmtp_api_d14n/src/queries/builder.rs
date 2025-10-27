//! Generic Builder for the backend API

use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use xmtp_api_grpc::GrpcClient;
use xmtp_api_grpc::error::{GrpcBuilderError, GrpcError};
use xmtp_common::RetryableError;
use xmtp_common::{MaybeSend, MaybeSync};
use xmtp_configuration::MULTI_NODE_TIMEOUT_MS;
use xmtp_id::scw_verifier::VerifierError;
use xmtp_proto::api::ApiClientError;
use xmtp_proto::api_client::ApiBuilder;
use xmtp_proto::types::AppVersion;

use crate::protocol::{CursorStore, FullXmtpApiArc, FullXmtpApiBox, FullXmtpApiT, NoCursorStore};
use crate::{
    AuthCallback, AuthHandle, AuthMiddleware, D14nClient, MiddlewareBuilder,
    MultiNodeClientBuilderError, V3Client,
};
mod impls;

/// Builder to access the backend XMTP API
/// Passing a gateway host implicitly enables decentralization.
#[derive(Clone, Default)]
pub struct MessageBackendBuilder {
    v3_host: Option<String>,
    gateway_host: Option<String>,
    app_version: Option<AppVersion>,
    cursor_store: Option<Arc<dyn CursorStore>>,
    auth_callback: Option<Arc<dyn AuthCallback>>,
    auth_handle: Option<AuthHandle>,
    is_secure: bool,
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
        self.app_version = Some(version.into());
        self
    }

    /// Specify the node host
    /// for d14n this is the replication node
    /// for v3 this is xmtp-node-go
    ///
    /// Required
    pub fn v3_host<S: AsRef<str>>(&mut self, host: S) -> &mut Self {
        self.v3_host = Some(host.as_ref().to_string());
        self
    }

    /// Specify the gateway host
    /// the gateway is a d14n-specific host
    /// specifying this fields implicitly enables decentralization
    ///
    /// Optional
    pub fn gateway_host<S: AsRef<str>>(&mut self, host: S) -> &mut Self {
        self.gateway_host = Some(host.as_ref().to_string());
        self
    }

    /// Specify the gateway host as an Option<T>
    /// the gateway is a d14n-specific host
    /// specifying this fields implicitly enables decentralization
    ///
    /// Optional
    pub fn maybe_gateway_host<S: AsRef<str>>(&mut self, gateway_host: Option<S>) -> &mut Self {
        self.gateway_host = gateway_host.map(|s| s.as_ref().to_string());
        self
    }

    /// Indicate that the connection should use TLS
    pub fn is_secure(&mut self, is_secure: bool) -> &mut Self {
        self.is_secure = is_secure;
        self
    }

    pub fn cursor_store(&mut self, store: impl CursorStore + 'static) -> &mut Self {
        self.cursor_store = Some(Arc::new(store) as Arc<_>);
        self
    }

    pub fn maybe_auth_callback(&mut self, callback: Option<Arc<dyn AuthCallback>>) -> &mut Self {
        self.auth_callback = callback;
        self
    }

    pub fn maybe_auth_handle(&mut self, handle: Option<AuthHandle>) -> &mut Self {
        self.auth_handle = handle;
        self
    }

    /// Build the client
    pub fn build(
        &mut self,
    ) -> Result<FullXmtpApiArc<ApiClientError<GrpcError>>, MessageBackendBuilderError> {
        let Self {
            v3_host,
            gateway_host,
            app_version,
            is_secure,
            auth_callback,
            auth_handle,
            cursor_store,
        } = self.clone();
        let v3_host = v3_host.ok_or(MessageBackendBuilderError::MissingV3Host)?;
        let cursor_store = cursor_store.unwrap_or(Arc::new(NoCursorStore) as Arc<dyn CursorStore>);

        if let Some(gateway) = gateway_host {
            let mut gateway_client_builder = GrpcClient::builder();
            gateway_client_builder.set_host(gateway);
            gateway_client_builder.set_tls(is_secure);

            if let Some(version) = app_version {
                gateway_client_builder.set_app_version(version)?;
            }

            let mut multi_node = crate::middleware::MultiNodeClientBuilder::default();
            multi_node.set_timeout(Duration::from_millis(MULTI_NODE_TIMEOUT_MS))?;
            multi_node.set_tls(is_secure);
            multi_node.set_gateway_builder(gateway_client_builder.clone())?;

            let gateway_client = gateway_client_builder.build()?;
            let multi_node = multi_node.build()?;
            if auth_callback.is_some() || auth_handle.is_some() {
                let auth_middleware =
                    AuthMiddleware::new(gateway_client, auth_callback, auth_handle);
                Ok(D14nClient::new(multi_node, auth_middleware, cursor_store)?.arced())
            } else {
                Ok(D14nClient::new(multi_node, gateway_client, cursor_store)?.arced())
            }
        } else {
            let mut v3_client = GrpcClient::builder();
            v3_client.set_host(v3_host);
            v3_client.set_tls(is_secure);
            if let Some(ref version) = app_version {
                v3_client.set_app_version(version.clone())?;
            }

            let v3_client = v3_client.build()?;
            let v3_client = V3Client::new(v3_client, cursor_store);
            tracing::info!("V3Client type: {}", std::any::type_name_of_val(&v3_client));
            Ok(v3_client.arced())
        }
    }
}

// TODO:d14n: Remove once D14n-only
/// Temporary standalone to produce a new ApiClient with a cached store
pub fn new_client_with_store<E>(
    api: Arc<dyn FullXmtpApiT<E>>,
    store: Arc<dyn CursorStore>,
) -> Result<Arc<dyn FullXmtpApiT<ApiClientError<GrpcError>>>, MessageBackendBuilderError>
where
    E: RetryableError + 'static,
{
    if let Some(c) = super::d14n::d14n_new_with_store(api.clone(), store.clone()) {
        return Ok(c);
    }
    if let Some(c) = super::v3::v3_new_with_store(api.clone(), store) {
        return Ok(c);
    }
    Err(MessageBackendBuilderError::CursorStoreNotReplaced(
        std::any::type_name_of_val(&api),
    ))
}
