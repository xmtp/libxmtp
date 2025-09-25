//! Generic Builder for the backend API

use std::sync::Arc;

use thiserror::Error;
use xmtp_api_grpc::error::GrpcBuilderError;
use xmtp_api_grpc::{GrpcClient, error::GrpcError};
use xmtp_proto::api_client::ToDynApi;
use xmtp_proto::api_client::{ApiBuilder, ArcedXmtpApi};
use xmtp_proto::{api::ApiClientError, types::AppVersion};

use crate::protocol::CursorStore;
use crate::{D14nClient, V3Client};

/// Builder to access the backend XMTP API
/// Passing a gateway host implicitly enables decentralization.
#[derive(Clone, Default)]
pub struct MessageBackendBuilder {
    node_host: Option<String>,
    gateway_host: Option<String>,
    app_version: Option<AppVersion>,
    cursor_store: Option<Arc<dyn CursorStore>>,
    is_secure: bool,
}

#[derive(Error, Debug)]
pub enum MessageBackendBuilderError {
    #[error("Node host is always required")]
    MissingNodeHost,
    #[error("Cursor store is always required")]
    MissingCursorStore,
    #[error(transparent)]
    GrpcBuilder(#[from] GrpcBuilderError),
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
    pub fn node_host<S: AsRef<str>>(&mut self, host: S) -> &mut Self {
        self.node_host = Some(host.as_ref().to_string());
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

    pub fn is_secure(&mut self, is_secure: bool) -> &mut Self {
        self.is_secure = is_secure;
        self
    }

    pub fn cursor_store(&mut self, store: impl CursorStore + 'static) -> &mut Self {
        self.cursor_store = Some(Arc::new(store) as Arc<_>);
        self
    }

    /// Build the client
    pub fn build(
        &mut self,
    ) -> Result<ArcedXmtpApi<ApiClientError<GrpcError>>, MessageBackendBuilderError> {
        let Self {
            node_host,
            gateway_host,
            app_version,
            is_secure,
            cursor_store,
        } = self.clone();
        let node_host = node_host.ok_or(MessageBackendBuilderError::MissingNodeHost)?;
        let cursor_store = cursor_store.ok_or(MessageBackendBuilderError::MissingCursorStore)?;

        let mut node_client = GrpcClient::builder();
        node_client.set_host(node_host);
        node_client.set_tls(is_secure);
        if let Some(ref version) = app_version {
            node_client.set_app_version(version.clone())?;
        }

        if let Some(gateway) = gateway_host {
            let mut gateway_client = GrpcClient::builder();
            gateway_client.set_host(gateway);
            gateway_client.set_tls(is_secure);
            if let Some(version) = app_version {
                gateway_client.set_app_version(version)?;
            }
            let node_client = node_client.build()?;
            let gateway_client = gateway_client.build()?;
            Ok(D14nClient::new(node_client, gateway_client, cursor_store).arced())
        } else {
            let node_client = node_client.build()?;
            Ok(V3Client::new(node_client, cursor_store).arced())
        }
    }
}
