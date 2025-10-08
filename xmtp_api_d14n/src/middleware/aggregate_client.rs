use crate::d14n::{GetNodes, HealthCheck};
use crate::traits::MiddlewareBuilder;
use futures::{StreamExt, future::join_all};
use prost::bytes::Bytes;
use std::collections::HashMap;
use std::num::NonZeroUsize;
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
    #[error("all node clients failed to build")]
    AllNodeClientsFailedToBuild,
    #[error("no nodes found")]
    NoNodesFound,
    #[error("no responsive nodes found")]
    NoResponsiveNodesFound,
    #[error("unresponsive node")]
    UnresponsiveNode,
}

/// From<AggregateClientError> for ApiClientError<E> is used to convert the AggregateClientError to an ApiClientError.
impl<E> From<AggregateClientError> for ApiClientError<E>
where
    E: std::error::Error + Send + Sync + 'static,
{
    fn from(value: AggregateClientError) -> ApiClientError<E> {
        ApiClientError::<E>::Other(Box::new(value))
    }
}

/// RetryableError for AggregateClientError is used to determine if the error is retryable.
/// Trait needed by the From<AggregateClientError> for ApiClientError<C> implementation.
impl RetryableError for AggregateClientError {
    fn is_retryable(&self) -> bool {
        use AggregateClientError::*;
        match self {
            AllNodeClientsFailedToBuild => false,
            NoNodesFound => false,
            NoResponsiveNodesFound => false,
            UnresponsiveNode => false,
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

    tracing::debug!("got nodes from gateway: {:?}", response.nodes);

    let max_concurrency = response.nodes.len();

    let mut clients = HashMap::new();

    let mut stream =
        futures::stream::iter(response.nodes.into_iter().map(|(node_id, url)| async move {
            let mut client_builder = GrpcClient::builder();
            let is_tls = url.parse::<url::Url>()?.scheme() == "https";
            client_builder.set_tls(is_tls);
            client_builder.set_host(url);
            let client = client_builder.build().await?;

            Ok::<_, Box<dyn std::error::Error + Send + Sync>>((node_id, client))
        }))
        .buffer_unordered(max_concurrency);

    while let Some(res) = stream.next().await {
        match res {
            Ok((node_id, client)) => {
                tracing::info!("built client for node {}", node_id);
                clients.insert(node_id, client);
            }
            Err(err) => {
                tracing::error!("failed to build client: {}", err);
            }
        }
    }

    tracing::debug!(
        "built clients for nodes: {:?}",
        clients
            .iter()
            .map(|(node_id, _)| *node_id)
            .collect::<Vec<_>>()
    );

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

            xmtp_common::time::timeout(timeout, endpoint.query(&client))
                .await
                .map_err(|_| {
                    ApiClientError::new(
                        ApiEndpoint::HealthCheck,
                        AggregateClientError::UnresponsiveNode,
                    )
                })
                .and_then(|r| {
                    r.map_err(|_| {
                        ApiClientError::new(
                            ApiEndpoint::HealthCheck,
                            AggregateClientError::UnresponsiveNode,
                        )
                    })
                })
                .map(|_| (node_id, client, start.elapsed().as_millis() as u64))
        }
    });

    let results = join_all(futures).await;

    let fastest_node = results
        .into_iter()
        .inspect(|res| {
            if let Err(e) = res {
                tracing::warn!("healthcheck failed: {}", e);
            }
        })
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
    timeout: Option<NonZeroUsize>,
}

/// AggregateClientError is used to wrap the errors from the aggregate client.
#[derive(Debug, Error)]
pub enum AggregateClientBuilderError {
    #[error("timeout must be greater than 0")]
    InvalidTimeout,
    #[error("gateway client is required")]
    MissingGatewayClient,
    #[error(transparent)]
    Api(#[from] ApiClientError<AggregateClientError>),
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
        let nodes = get_nodes(&gateway_client)
            .await
            .map_err(AggregateClientBuilderError::from)?;

        let timeout = Duration::from_millis(
            self.timeout
                .ok_or(AggregateClientBuilderError::InvalidTimeout)?
                .get() as u64,
        );
        let inner = get_fastest_node(nodes, timeout)
            .await
            .map_err(AggregateClientBuilderError::from)?;

        Ok(AggregateClient {
            gateway_client,
            inner,
            timeout,
        })
    }
}
