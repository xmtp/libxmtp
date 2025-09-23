//! Generic Builder for the backend API

use std::time::Duration;
use thiserror::Error;
use xmtp_api_grpc::error::GrpcBuilderError;
use xmtp_api_grpc::{GrpcClient, error::GrpcError};
use xmtp_proto::api_client::ToDynApi;
use xmtp_proto::api_client::{ApiBuilder, ArcedXmtpApi};
use xmtp_proto::{api::ApiClientError, types::AppVersion};

use crate::{
    D14nClient, MiddlewareBuilder, MultiNodeClientBuilder, MultiNodeClientBuilderError, V3Client,
};

/// Builder to access the backend XMTP API
/// Passing a gateway host implicitly enables decentralization.
#[derive(Clone, Default)]
pub struct MessageBackendBuilder {
    v3_host: Option<String>,
    gateway_host: Option<String>,
    app_version: Option<AppVersion>,
    is_secure: bool,
}

#[derive(Error, Debug)]
pub enum MessageBackendBuilderError {
    #[error("V3 Host is required")]
    MissingV3Host,
    #[error(transparent)]
    GrpcBuilder(#[from] GrpcBuilderError),
    #[error(transparent)]
    MutliNode(#[from] MultiNodeClientBuilderError),
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

    /// Build the client
    pub fn build(
        &mut self,
    ) -> Result<ArcedXmtpApi<ApiClientError<GrpcError>>, MessageBackendBuilderError> {
        let Self {
            v3_host,
            gateway_host,
            app_version,
            is_secure,
        } = self.clone();
        let v3_host = v3_host.ok_or(MessageBackendBuilderError::MissingV3Host)?;

        if let Some(gateway) = gateway_host {
            let mut gateway_client = GrpcClient::builder();
            gateway_client.set_host(gateway);
            gateway_client.set_tls(is_secure);
            if let Some(version) = app_version {
                gateway_client.set_app_version(version)?;
            }
            let gateway_client = gateway_client.build()?;
            let mut multi_node = crate::multi_node::builder();
            multi_node.set_timeout(Duration::from_millis(100))?;
            multi_node.set_tls(true);
            multi_node.set_gateway_client(gateway_client.clone())?;
            let multi_node = <MultiNodeClientBuilder as ApiBuilder>::build(multi_node)?;

            Ok(D14nClient::new(multi_node, gateway_client).arced())
        } else {
            let mut v3_client = GrpcClient::builder();
            v3_client.set_host(v3_host);
            v3_client.set_tls(is_secure);
            if let Some(ref version) = app_version {
                v3_client.set_app_version(version.clone())?;
            }

            let v3_client = v3_client.build()?;
            Ok(V3Client::new(v3_client).arced())
        }
    }
}
