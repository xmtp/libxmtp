//! Generic Builder for the backend API

use thiserror::Error;
use xmtp_api_grpc::error::GrpcBuilderError;
use xmtp_api_grpc::{GrpcClient, error::GrpcError};
use xmtp_proto::api_client::ToDynApi;
use xmtp_proto::api_client::{ApiBuilder, ArcedXmtpApiWithStreams};
use xmtp_proto::{api::ApiClientError, types::AppVersion};

use crate::{D14nClient, V3Client};

/// Builder to access the backend XMTP API
/// Passing a payer host implicitly enables decentralization.
#[derive(Clone, Default)]
pub struct MessageBackendBuilder {
    node_host: Option<String>,
    payer_host: Option<String>,
    app_version: Option<AppVersion>,
    is_secure: bool,
}

#[derive(Error, Debug)]
pub enum MessageBackendBuilderError {
    #[error("Node host is always required")]
    MissingNodeHost,
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

    /// Specify the payer host
    /// the payer is a d14n-specific host
    /// specifying this fields implicitly enables decentralization
    ///
    /// Optional
    pub fn payer_host<S: AsRef<str>>(&mut self, host: S) -> &mut Self {
        self.payer_host = Some(host.as_ref().to_string());
        self
    }

    /// Specify the payer host as an Option<T>
    /// the payer is a d14n-specific host
    /// specifying this fields implicitly enables decentralization
    ///
    /// Optional
    pub fn maybe_payer_host<S: AsRef<str>>(&mut self, payer_host: Option<S>) -> &mut Self {
        self.payer_host = payer_host.map(|s| s.as_ref().to_string());
        self
    }

    pub fn is_secure(&mut self, is_secure: bool) -> &mut Self {
        self.is_secure = is_secure;
        self
    }

    /// Build the client
    pub async fn build(
        &mut self,
    ) -> Result<ArcedXmtpApiWithStreams<ApiClientError<GrpcError>>, MessageBackendBuilderError>
    {
        let Self {
            node_host,
            payer_host,
            app_version,
            is_secure,
        } = self.clone();
        let node_host = node_host.ok_or(MessageBackendBuilderError::MissingNodeHost)?;

        let mut node_client = GrpcClient::builder();
        node_client.set_host(node_host);
        node_client.set_tls(is_secure);
        if let Some(ref version) = app_version {
            node_client.set_app_version(version.clone())?;
        }

        if let Some(payer) = payer_host {
            let mut payer_client = GrpcClient::builder();
            payer_client.set_host(payer);
            payer_client.set_tls(is_secure);
            if let Some(version) = app_version {
                payer_client.set_app_version(version)?;
            }
            let node_client = node_client.build().await?;
            let payer_client = payer_client.build().await?;
            Ok(D14nClient::new(node_client, payer_client).arced())
        } else {
            let node_client = node_client.build().await?;
            Ok(V3Client::new(node_client).arced())
        }
    }
}
