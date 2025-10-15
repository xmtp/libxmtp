use crate::d14n::{GetNodes, HealthCheck};
use futures::StreamExt;
use prost::bytes::Bytes;
use std::collections::HashMap;
use std::num::NonZeroUsize;
use thiserror::Error;
use xmtp_api_grpc::{client::GrpcClient, error::GrpcError};
use xmtp_common::{
    RetryableError,
    time::{Duration, Instant},
};
use xmtp_proto::{
    ApiEndpoint,
    api::{ApiClientError, BodyError, Client, Query},
    prelude::ApiBuilder,
};

/* AggregateClient struct and impls */

#[derive(Debug, Error)]
pub enum AggregateClientError {
    #[error("all node clients failed to build")]
    AllNodeClientsFailedToBuild,
    #[error(transparent)]
    BodyError(#[from] BodyError),
    #[error(transparent)]
    GrpcError(#[from] ApiClientError<GrpcError>),
    #[error("no nodes found")]
    NoNodesFound,
    #[error("no responsive nodes found under {latency}ms latency")]
    NoResponsiveNodesFound { latency: u64 },
    #[error("timeout reaching node {} under {}ms latency", node_id, latency)]
    TimeoutNode { node_id: u32, latency: u64 },
    #[error("unhealthy node {}", node_id)]
    UnhealthyNode { node_id: u32 },
}

/// From<AggregateClientError> for ApiClientError<E> is used to convert the AggregateClientError to an ApiClientError.
/// Required by the Client trait implementation, as request and stream can return AggregateClientError.
impl<E> From<AggregateClientError> for ApiClientError<E>
where
    E: std::error::Error + Send + Sync + 'static,
{
    fn from(value: AggregateClientError) -> ApiClientError<E> {
        ApiClientError::<E>::Other(Box::new(value))
    }
}

/// RetryableError for AggregateClientError is used to determine if the error is retryable.
/// Trait needed by the From<AggregateClientError> for ApiClientError<E> implementation.
/// All errors are not retryable.
impl RetryableError for AggregateClientError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::GrpcError(e) => e.is_retryable(),
            Self::BodyError(e) => e.is_retryable(),
            _ => false,
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
    pub async fn refresh(&mut self) -> Result<(), ApiClientError<AggregateClientError>> {
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
    }

    async fn stream(
        &self,
        request: http::request::Builder,
        path: http::uri::PathAndQuery,
        body: Bytes,
    ) -> Result<http::Response<Self::Stream>, ApiClientError<Self::Error>> {
        self.inner.stream(request, path, body).await
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
        .map_err(|e| {
            tracing::error!("failed to get nodes from gateway: {}", e);
            ApiClientError::new(ApiEndpoint::GetNodes, AggregateClientError::GrpcError(e))
        })?;

    let max_concurrency = if response.nodes.is_empty() {
        tracing::warn!("no nodes found");
        Err(ApiClientError::new(
            ApiEndpoint::GetNodes,
            AggregateClientError::NoNodesFound,
        ))
    } else {
        Ok(response.nodes.len())
    }?;

    tracing::debug!("got nodes from gateway: {:?}", response.nodes);

    let mut clients: HashMap<u32, GrpcClient> = HashMap::new();

    let mut stream =
        futures::stream::iter(response.nodes.into_iter().map(|(node_id, url)| async move {
            tracing::debug!("building client for node {}: {}", node_id, url);
            let mut client_builder = GrpcClient::builder();

            let is_tls = url
                .parse::<url::Url>()
                .map_err(|e| (node_id, e.into()))?
                .scheme()
                == "https";

            client_builder.set_tls(is_tls);
            client_builder.set_host(url);

            let client = client_builder.build().map_err(|e| (node_id, e.into()))?;

            Ok::<_, (u32, Box<dyn std::error::Error + Send + Sync>)>((node_id, client))
        }))
        .buffer_unordered(max_concurrency);

    while let Some(res) = stream.next().await {
        match res {
            Ok((node_id, client)) => {
                tracing::info!("built client for node {}", node_id);
                clients.insert(node_id, client);
            }
            Err(err) => {
                tracing::error!("failed to build client for node {}: {}", err.0, err.1);
            }
        }
    }

    if clients.is_empty() {
        tracing::error!("all node clients failed to build");
        return Err(ApiClientError::new(
            ApiEndpoint::GetNodes,
            AggregateClientError::AllNodeClientsFailedToBuild,
        ));
    }

    tracing::debug!("built clients for nodes: {:?}", clients.keys());

    Ok(clients)
}

/// Get the fastest node from the list of endpoints.
async fn get_fastest_node(
    clients: HashMap<u32, GrpcClient>,
    timeout: Duration,
) -> Result<GrpcClient, ApiClientError<AggregateClientError>> {
    let endpoint = HealthCheck::builder().build().map_err(|e| {
        tracing::error!("failed to build healthcheck endpoint: {}", e);
        ApiClientError::new(ApiEndpoint::HealthCheck, AggregateClientError::BodyError(e))
    })?;

    let max_concurrency = if clients.is_empty() {
        tracing::warn!("no nodes found");
        Err(ApiClientError::Other(Box::new(
            AggregateClientError::NoNodesFound,
        )))
    } else {
        Ok(clients.len())
    }?;

    let mut fastest_client: Option<(u32, GrpcClient, u64)> = None;

    let mut stream = futures::stream::iter(clients.into_iter().map(|(node_id, client)| {
        let mut endpoint = endpoint.clone();

        async move {
            tracing::debug!("healthcheck node {}", node_id);

            let start = Instant::now();

            xmtp_common::time::timeout(timeout, endpoint.query(&client))
                .await
                .map_err(|_| {
                    tracing::error!("node timed out: {}", node_id);
                    ApiClientError::new(
                        ApiEndpoint::HealthCheck,
                        AggregateClientError::TimeoutNode {
                            node_id,
                            latency: timeout.as_millis() as u64,
                        },
                    )
                })
                .and_then(|r| {
                    r.map_err(|_| {
                        tracing::error!("node is unhealthy: {}", node_id);
                        ApiClientError::new(
                            ApiEndpoint::HealthCheck,
                            AggregateClientError::UnhealthyNode { node_id },
                        )
                    })
                })
                .map(|_| (node_id, client, start.elapsed().as_millis() as u64))
        }
    }))
    .buffer_unordered(max_concurrency);

    while let Some(res) = stream.next().await {
        match res {
            Ok((node_id, client, latency)) => {
                if fastest_client
                    .as_ref()
                    .map(|f| latency < f.2)
                    .unwrap_or(true)
                {
                    fastest_client = Some((node_id, client, latency));
                }
            }
            Err(e) => {
                tracing::warn!("healthcheck failed: {}", e);
            }
        }
    }

    let (_, client, _) = fastest_client.ok_or(ApiClientError::new(
        ApiEndpoint::HealthCheck,
        AggregateClientError::NoResponsiveNodesFound {
            latency: timeout.as_millis() as u64,
        },
    ))?;

    Ok(client)
}

/* MiddlewareBuilder implementation for AggregateClient */

#[derive(Default)]
pub struct AggregateClientBuilder<C>
where
    C: Client + Sync + Send,
{
    gateway_client: Option<C>,
    timeout: Option<NonZeroUsize>,
}

/// AggregateClientError is used to wrap the errors from the aggregate client.
#[derive(Debug, Error)]
pub enum AggregateClientBuilderError {
    #[error(transparent)]
    ApiError(#[from] Box<ApiClientError<AggregateClientError>>),
    #[error("timeout must be greater than 0")]
    InvalidTimeout,
    #[error("gateway client is required")]
    MissingGatewayClient,
}

impl MiddlewareBuilder<GrpcClient> for AggregateClientBuilder<GrpcClient> {
    type Output = AggregateClient<GrpcClient>;
    type Error = AggregateClientBuilderError;

    fn set_gateway_client(&mut self, gateway_client: GrpcClient) -> Result<(), Self::Error> {
        self.gateway_client = Some(gateway_client);
        Ok(())
    }

    fn set_timeout(&mut self, timeout: NonZeroUsize) -> Result<(), Self::Error> {
        self.timeout = Some(timeout);
        Ok(())
    }

    async fn build(self) -> Result<Self::Output, Self::Error> {
        let gateway_client = self
            .gateway_client
            .ok_or(AggregateClientBuilderError::MissingGatewayClient)?;
        let nodes = get_nodes(&gateway_client).await.map_err(Box::new)?;

        let timeout = Duration::from_millis(self.timeout.unwrap().get() as u64);
        let inner = get_fastest_node(nodes, timeout).await.map_err(Box::new)?;

        Ok(AggregateClient {
            gateway_client,
            inner,
            timeout,
        })
    }
}

/* MiddlewareBuilder */

pub trait MiddlewareBuilder<C>
where
    C: Client + Sync + Send,
{
    type Output;
    type Error;

    /// set the gateway client for node discovery
    fn set_gateway_client(&mut self, gateway_client: C) -> Result<(), Self::Error>;

    /// max timeout allowed for nodes to respond, in milliseconds
    fn set_timeout(&mut self, timeout: NonZeroUsize) -> Result<(), Self::Error>;

    #[allow(async_fn_in_trait)]
    async fn build(self) -> Result<Self::Output, Self::Error>;
}
