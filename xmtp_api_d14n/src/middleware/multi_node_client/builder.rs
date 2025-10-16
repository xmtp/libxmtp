use crate::{MultiNodeClient, middleware::MiddlewareBuilder};
use thiserror::Error;
use tokio::sync::OnceCell;
use xmtp_api_grpc::{ClientBuilder, GrpcClient, error::GrpcBuilderError};
use xmtp_common::time::Duration;
use xmtp_proto::{api_client::ApiBuilder, types::AppVersion};

/* MultiNodeClientBuilder struct and its associated errors */

pub struct MultiNodeClientBuilder {
    pub gateway_client: Option<GrpcClient>,
    pub timeout: Duration,
    pub node_client_template: ClientBuilder,
}

/// Errors that can occur when building a MultiNodeClient.
#[derive(Debug, Error)]
pub enum MultiNodeClientBuilderError {
    #[error(transparent)]
    GrpcBuilderError(#[from] GrpcBuilderError),
    #[error("timeout must be greater than 0")]
    InvalidTimeout,
    #[error("gateway client is required")]
    MissingGatewayClient,
}

/* MultiNodeClientBuilder implementations */

// Default implementation for MultiNodeClientBuilder.
// Allows invoking MultiNodeClientBuilder::default() to create a new builder with default values.
impl Default for MultiNodeClientBuilder {
    fn default() -> Self {
        Self {
            gateway_client: None,
            timeout: Duration::from_millis(1000),
            node_client_template: GrpcClient::builder(),
        }
    }
}

// Implement the MiddlewareBuilder trait for MultiNodeClientBuilder.
// This defines how to build a MultiNodeClient from a MultiNodeClientBuilder.
impl MiddlewareBuilder for MultiNodeClientBuilder {
    type Output = MultiNodeClient;
    type Error = MultiNodeClientBuilderError;

    fn set_gateway_client(&mut self, gateway_client: GrpcClient) -> Result<(), Self::Error> {
        self.gateway_client = Some(gateway_client);
        Ok(())
    }

    fn set_timeout(&mut self, timeout: Duration) -> Result<(), Self::Error> {
        self.timeout = timeout;
        Ok(())
    }

    fn build(self) -> Result<Self::Output, Self::Error> {
        let gateway_client = self
            .gateway_client
            .ok_or(MultiNodeClientBuilderError::MissingGatewayClient)?;

        if self.timeout.is_zero() {
            return Err(MultiNodeClientBuilderError::InvalidTimeout);
        }

        Ok(MultiNodeClient {
            gateway_client,
            inner: OnceCell::new(),
            timeout: self.timeout,
            node_client_template: self.node_client_template,
        })
    }
}

// Implement the ApiBuilder trait for MultiNodeClientBuilder.
// This allows the MultiNodeClientBuilder to be passed as parameter to the D14nClientBuilder.
impl ApiBuilder for MultiNodeClientBuilder {
    type Output = MultiNodeClient;
    type Error = MultiNodeClientBuilderError;

    fn set_libxmtp_version(&mut self, version: String) -> Result<(), Self::Error> {
        ClientBuilder::set_libxmtp_version(&mut self.node_client_template, version)?;
        Ok(())
    }

    fn set_app_version(&mut self, version: AppVersion) -> Result<(), Self::Error> {
        ClientBuilder::set_app_version(&mut self.node_client_template, version)?;
        Ok(())
    }

    /// No-op: node hosts are discovered dynamically via the gateway.
    fn set_host(&mut self, _: String) {}

    fn set_tls(&mut self, tls: bool) {
        ClientBuilder::set_tls(&mut self.node_client_template, tls);
    }

    fn set_retry(&mut self, retry: xmtp_common::Retry) {
        ClientBuilder::set_retry(&mut self.node_client_template, retry);
    }

    fn rate_per_minute(&mut self, limit: u32) {
        ClientBuilder::rate_per_minute(&mut self.node_client_template, limit);
    }

    fn port(&self) -> Result<Option<String>, Self::Error> {
        ClientBuilder::port(&self.node_client_template)
            .map(|_| None)
            .map_err(Into::into)
    }

    fn host(&self) -> Option<&str> {
        ClientBuilder::host(&self.node_client_template)
    }

    fn build(self) -> Result<Self::Output, Self::Error> {
        let gateway_client = self
            .gateway_client
            .ok_or(MultiNodeClientBuilderError::MissingGatewayClient)?;

        Ok(MultiNodeClient {
            gateway_client,
            inner: OnceCell::new(),
            timeout: self.timeout,
            node_client_template: self.node_client_template,
        })
    }
}
