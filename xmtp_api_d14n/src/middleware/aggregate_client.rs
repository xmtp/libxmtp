use crate::d14n::{GetNodes, HealthCheck};
use crate::traits::MiddlewareBuilder;
use futures::future::join_all;
use prost::bytes::Bytes;
use std::collections::HashMap;
use thiserror::Error;
use xmtp_api_grpc::GrpcClient;
use xmtp_common::{
    RetryableError,
    time::{Duration, Instant},
};
use xmtp_proto::{
    ApiEndpoint,
    prelude::ApiBuilder,
    traits::{ApiClientError, Client, Query},
};

/// AggregateClientError is used to wrap the errors from the aggregate client.
#[derive(Debug, Error)]
pub enum AggregateClientError {
    #[error("timeout must be greater than 0")]
    InvalidTimeout,
    #[error("no nodes found")]
    NoNodesFound,
    #[error("all node clients failed to build")]
    AllNodeClientsFailedToBuild,
    #[error("no responsive nodes found")]
    NoResponsiveNodesFound,
}

/// From<AggregateClientError> for ApiClientError<C> is used to convert the AggregateClientError to an ApiClientError.
impl<C> From<AggregateClientError> for ApiClientError<C>
where
    C: std::error::Error + Send + Sync + 'static,
{
    fn from(value: AggregateClientError) -> ApiClientError<C> {
        ApiClientError::<C>::Other(Box::new(value))
    }
}

/// RetryableError for AggregateClientError is used to determine if the error is retryable.
/// Trait needed by the From<AggregateClientError> for ApiClientError<C> implementation.
impl RetryableError for AggregateClientError {
    fn is_retryable(&self) -> bool {
        use AggregateClientError::*;
        match self {
            InvalidTimeout => false,
            NoNodesFound => false,
            AllNodeClientsFailedToBuild => false,
            NoResponsiveNodesFound => false,
        }
    }
}

pub struct AggregateClient<C>
where
    C: Client + Sync + Send,
{
    gateway_client: C,
    inner: C,
    timeout: Duration,
}

impl AggregateClient<GrpcClient> {
    /// refresh checks the fastest node and updates the inner client
    /// should only be called when there are no active requests or streams
    pub async fn refresh(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let nodes = get_nodes(&self.gateway_client).await?;
        self.inner = get_fastest_node(nodes, self.timeout).await?;
        Ok(())
    }
}

/// Implement the Client trait for the AggregateClient.
/// This allows the AggregateClient to be used as a Client for any endpoint.
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<C> Client for AggregateClient<C>
where
    C: Client + Sync + Send,
{
    type Error = C::Error;
    type Stream = C::Stream;

    async fn request(
        &self,
        request: http::request::Builder,
        path: http::uri::PathAndQuery,
        body: Bytes,
    ) -> Result<http::Response<Bytes>, ApiClientError<Self::Error>> {
        self.inner.request(request, path, body).await
        // TODO: Refresh if performance is bad
    }

    async fn stream(
        &self,
        request: http::request::Builder,
        path: http::uri::PathAndQuery,
        body: Bytes,
    ) -> Result<http::Response<Self::Stream>, ApiClientError<Self::Error>> {
        self.inner.stream(request, path, body).await
        // TODO: Refresh if performance is bad
    }
}

/// Get the nodes from the gateway server.
async fn get_nodes(
    gateway_client: &GrpcClient,
) -> Result<HashMap<u32, GrpcClient>, ApiClientError<AggregateClientError>> {
    let response = GetNodes::builder()
        .build()?
        .query(gateway_client)
        .await
        .map_err(|_| {
            ApiClientError::new(ApiEndpoint::GetNodes, AggregateClientError::NoNodesFound)
        })?;

    let futures = response.nodes.into_iter().map(|(node_id, url)| async move {
        let mut client_builder = GrpcClient::builder();
        let is_tls = url.parse::<url::Url>()?.scheme() == "https";

        client_builder.set_tls(is_tls);
        client_builder.set_host(url);

        let client = client_builder.build().await?;

        Ok::<_, Box<dyn std::error::Error + Send + Sync>>((node_id, client))
    });

    let results = join_all(futures).await;

    let mut clients = HashMap::new();
    for result in results {
        match result {
            Ok((node_id, client)) => {
                clients.insert(node_id, client);
            }
            Err(err) => {
                tracing::warn!("failed to build client: {}", err);
            }
        }
    }

    if clients.is_empty() {
        return Err(ApiClientError::new(
            ApiEndpoint::GetNodes,
            AggregateClientError::AllNodeClientsFailedToBuild,
        ));
    }

    Ok(clients)
}

/// Get the fastest node from the list of endpoints.
async fn get_fastest_node(
    clients: HashMap<u32, GrpcClient>,
    timeout: Duration,
) -> Result<GrpcClient, ApiClientError<AggregateClientError>> {
    let endpoint = HealthCheck::builder().build()?;

    let futures = clients.into_iter().map(|(node_id, client)| {
        let endpoint = endpoint.clone();
        async move {
            let start = Instant::now();
            let result = xmtp_common::time::timeout(timeout, endpoint.query(&client)).await;

            match result {
                Ok(Ok(_)) => Some((node_id, client, start.elapsed().as_millis() as u64)),
                _ => None,
            }
        }
    });

    let results = join_all(futures).await;

    let fastest_node = results
        .into_iter()
        .flatten()
        .min_by_key(|(_, _, latency)| *latency)
        .ok_or(ApiClientError::new(
            ApiEndpoint::HealthCheck,
            AggregateClientError::NoResponsiveNodesFound,
        ))?;

    Ok(fastest_node.1)
}

#[derive(Default)]
pub struct AggregateClientBuilder<C>
where
    C: Client + Sync + Send,
{
    gateway_client: Option<C>,
    timeout: Option<Duration>,
}

/// From<AggregateClientError> for ApiClientError<C> is used to convert the AggregateClientError to an ApiClientError.
impl From<AggregateClientError> for AggregateClientBuilderError {
    fn from(value: AggregateClientError) -> AggregateClientBuilderError {
        AggregateClientBuilderError::Other(Box::new(value))
    }
}

/// AggregateClientError is used to wrap the errors from the aggregate client.
#[derive(Debug, Error)]
pub enum AggregateClientBuilderError {
    #[error("timeout must be greater than 0")]
    InvalidTimeout,
    #[error("gateway client is required")]
    MissingGatewayClient,
    #[error(transparent)]
    Other(Box<dyn std::error::Error + Send + Sync>),
}

impl MiddlewareBuilder<GrpcClient> for AggregateClientBuilder<GrpcClient> {
    type Output = AggregateClient<GrpcClient>;
    type Error = AggregateClientBuilderError;

    fn set_gateway_client(&mut self, gateway_client: GrpcClient) -> Result<(), Self::Error> {
        self.gateway_client = Some(gateway_client);
        Ok(())
    }

    fn set_timeout(&mut self, timeout: Duration) -> Result<(), Self::Error> {
        match timeout {
            Duration::ZERO => return Err(AggregateClientBuilderError::InvalidTimeout),
            _ => self.timeout = Some(timeout),
        }
        Ok(())
    }

    async fn build(self) -> Result<Self::Output, Self::Error> {
        let gateway_client = self
            .gateway_client
            .ok_or(AggregateClientBuilderError::MissingGatewayClient)?;
        let nodes = get_nodes(&gateway_client)
            .await
            .map_err(|e| AggregateClientBuilderError::Other(e.into()))?;

        let timeout = self
            .timeout
            .ok_or(AggregateClientBuilderError::InvalidTimeout)?;
        let inner = get_fastest_node(nodes, timeout)
            .await
            .map_err(|e| AggregateClientBuilderError::Other(e.into()))?;

        Ok(AggregateClient {
            gateway_client,
            inner,
            timeout,
        })
    }
}
