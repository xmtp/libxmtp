use crate::{MultiNodeClient, middleware::MiddlewareBuilder};
use thiserror::Error;
use tokio::sync::OnceCell;
use xmtp_api_grpc::{ClientBuilder, GrpcClient, error::GrpcBuilderError};
use xmtp_common::time::Duration;
use xmtp_configuration::MULTI_NODE_TIMEOUT_MS;
use xmtp_proto::api_client::ApiBuilder;

/* MultiNodeClientBuilder struct and its associated errors */

pub struct MultiNodeClientBuilder {
    gateway_builder: Option<ClientBuilder>,
    timeout: Duration,
    node_client_template: ClientBuilder,
}

/// Errors that can occur when building a MultiNodeClient.
#[derive(Debug, Error)]
pub enum MultiNodeClientBuilderError {
    #[error(transparent)]
    GrpcBuilderError(#[from] GrpcBuilderError),
    #[error("timeout must be greater than 0")]
    InvalidTimeout,
    #[error("gateway builder is required")]
    MissingGatewayBuilder,
}

/* MultiNodeClientBuilder implementations */

// Default implementation for MultiNodeClientBuilder.
// Allows invoking MultiNodeClientBuilder::default() to create a new builder with default values.
impl Default for MultiNodeClientBuilder {
    fn default() -> Self {
        Self {
            gateway_builder: None,
            timeout: Duration::from_millis(MULTI_NODE_TIMEOUT_MS),
            node_client_template: GrpcClient::builder(),
        }
    }
}

// Implement the MiddlewareBuilder trait for MultiNodeClientBuilder.
// This defines how to build a MultiNodeClient from a MultiNodeClientBuilder.
impl MiddlewareBuilder for MultiNodeClientBuilder {
    fn set_gateway_builder(&mut self, gateway_builder: ClientBuilder) -> Result<(), Self::Error> {
        self.gateway_builder = Some(gateway_builder);
        Ok(())
    }

    fn set_node_client_builder(&mut self, node_builder: ClientBuilder) -> Result<(), Self::Error> {
        self.node_client_template = node_builder;
        Ok(())
    }

    fn set_timeout(&mut self, timeout: Duration) -> Result<(), Self::Error> {
        self.timeout = timeout;
        Ok(())
    }
}

// Implement the ApiBuilder trait for MultiNodeClientBuilder.
// This allows the MultiNodeClientBuilder to be passed as parameter to the D14nClientBuilder.
impl ApiBuilder for MultiNodeClientBuilder {
    type Output = MultiNodeClient;
    type Error = MultiNodeClientBuilderError;

    fn build(self) -> Result<Self::Output, Self::Error> {
        let gateway_builder = self
            .gateway_builder
            .ok_or(MultiNodeClientBuilderError::MissingGatewayBuilder)?;

        if self.timeout.is_zero() {
            return Err(MultiNodeClientBuilderError::InvalidTimeout);
        }

        let gateway_client = gateway_builder.build()?;

        Ok(MultiNodeClient {
            gateway_client,
            inner: OnceCell::new(),
            timeout: self.timeout,
            node_client_template: self.node_client_template,
        })
    }
}
