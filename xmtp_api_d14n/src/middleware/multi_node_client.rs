use crate::d14n::{GetNodes, HealthCheck};
use futures::StreamExt;
use prost::bytes::Bytes;
use std::collections::HashMap;
use thiserror::Error;
use tokio::sync::OnceCell;
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

/* MultiNodeClient struct and impls */

#[derive(Debug, Error)]
pub enum MultiNodeClientError {
    #[error("all node clients failed to build")]
    AllNodeClientsFailedToBuild,
    #[error(transparent)]
    BodyError(#[from] BodyError),
    #[error(transparent)]
    GrpcError(#[from] ApiClientError<GrpcError>),
    #[error("node {} timed out under {}ms latency", node_id, latency)]
    NodeTimedOut { node_id: u32, latency: u64 },
    #[error("no nodes found")]
    NoNodesFound,
    #[error("no responsive nodes found under {latency}ms latency")]
    NoResponsiveNodesFound { latency: u64 },
    #[error("client builder tls channel does not match url tls channel")]
    TlsChannelMismatch {
        url_is_tls: bool,
        client_builder_tls_channel: bool,
    },
    #[error("unhealthy node {}", node_id)]
    UnhealthyNode { node_id: u32 },
}

/// From<MultiNodeClientError> for ApiClientError<E> is used to convert the MultiNodeClientError to an ApiClientError.
/// Required by the Client trait implementation, as request and stream can return MultiNodeClientError.
impl<E> From<MultiNodeClientError> for ApiClientError<E>
where
    E: std::error::Error + Send + Sync + 'static,
{
    fn from(value: MultiNodeClientError) -> ApiClientError<E> {
        ApiClientError::<E>::Other(Box::new(value))
    }
}

/// RetryableError for MultiNodeClientError is used to determine if the error is retryable.
/// Trait needed by the From<MultiNodeClientError> for ApiClientError<E> implementation.
/// All errors are not retryable.
impl RetryableError for MultiNodeClientError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::GrpcError(e) => e.is_retryable(),
            Self::BodyError(e) => e.is_retryable(),
            _ => false,
        }
    }
}

pub struct MultiNodeClient {
    gateway_client: GrpcClient,
    inner: OnceCell<GrpcClient>,
    timeout: Duration,
    node_client_template: xmtp_api_grpc::client::ClientBuilder,
}

// TODO: Future PR implements a refresh() method that updates the inner client.
// In order to do so we need to use an OnceCell<ArcSwap<GrpcClient>>, so that
// we can update swap the inner client inside an OnceCell.
impl MultiNodeClient {
    async fn init_inner(&self) -> Result<&GrpcClient, ApiClientError<MultiNodeClientError>> {
        self.inner
            .get_or_try_init(|| async {
                let nodes = get_nodes(&self.gateway_client, &self.node_client_template).await?;
                let fastest_node = get_fastest_node(nodes, self.timeout).await?;
                Ok(fastest_node)
            })
            .await
    }
}

/// Implement the Client trait for the MultiNodeClient.
/// This allows the MultiNodeClient to be used as a Client for any endpoint.
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Client for MultiNodeClient {
    type Error = GrpcError;
    type Stream = <GrpcClient as Client>::Stream;

    async fn request(
        &self,
        request: http::request::Builder,
        path: http::uri::PathAndQuery,
        body: Bytes,
    ) -> Result<http::Response<Bytes>, ApiClientError<Self::Error>> {
        let inner = self
            .init_inner()
            .await
            .map_err(|e| ApiClientError::<GrpcError>::Other(Box::new(e)))?;

        inner.request(request, path, body).await
    }

    async fn stream(
        &self,
        request: http::request::Builder,
        path: http::uri::PathAndQuery,
        body: Bytes,
    ) -> Result<http::Response<Self::Stream>, ApiClientError<Self::Error>> {
        let inner = self
            .init_inner()
            .await
            .map_err(|e| ApiClientError::<GrpcError>::Other(Box::new(e)))?;

        inner.stream(request, path, body).await
    }
}

/// Get the nodes from the gateway server.
async fn get_nodes(
    gateway_client: &GrpcClient,
    template: &xmtp_api_grpc::client::ClientBuilder,
) -> Result<HashMap<u32, GrpcClient>, ApiClientError<MultiNodeClientError>> {
    let response = GetNodes::builder()
        .build()?
        .query(gateway_client)
        .await
        .map_err(|e| {
            tracing::error!("failed to get nodes from gateway: {}", e);
            ApiClientError::new(ApiEndpoint::GetNodes, MultiNodeClientError::GrpcError(e))
        })?;

    let max_concurrency = if response.nodes.is_empty() {
        tracing::warn!("no nodes found");
        Err(ApiClientError::new(
            ApiEndpoint::GetNodes,
            MultiNodeClientError::NoNodesFound,
        ))
    } else {
        Ok(response.nodes.len())
    }?;

    tracing::debug!("got nodes from gateway: {:?}", response.nodes);

    let mut clients: HashMap<u32, GrpcClient> = HashMap::new();

    let mut stream =
        futures::stream::iter(response.nodes.into_iter().map(|(node_id, url)| async move {
            tracing::debug!("building client for node {}: {}", node_id, url);

            let mut client_builder = template.clone();

            let url_is_tls = url
                .parse::<url::Url>()
                .map_err(|e| (node_id, e.into()))?
                .scheme()
                == "https";

            // Fail when the builder template tls channel does not match the url tls channel.
            // Template options takes precedence over the url tls channel.
            (client_builder.tls_channel == url_is_tls)
                .then_some(())
                .ok_or_else(|| {
                    (
                        node_id,
                        Box::new(MultiNodeClientError::TlsChannelMismatch {
                            url_is_tls,
                            client_builder_tls_channel: client_builder.tls_channel,
                        }) as Box<dyn std::error::Error + Send + Sync>,
                    )
                })?;

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
            MultiNodeClientError::AllNodeClientsFailedToBuild,
        ));
    }

    tracing::debug!("built clients for nodes: {:?}", clients.keys());

    Ok(clients)
}

/// Get the fastest node from the list of endpoints.
async fn get_fastest_node(
    clients: HashMap<u32, GrpcClient>,
    timeout: Duration,
) -> Result<GrpcClient, ApiClientError<MultiNodeClientError>> {
    let endpoint = HealthCheck::builder().build().map_err(|e| {
        tracing::error!("failed to build healthcheck endpoint: {}", e);
        ApiClientError::new(ApiEndpoint::HealthCheck, MultiNodeClientError::BodyError(e))
    })?;

    let max_concurrency = if clients.is_empty() {
        tracing::warn!("no nodes found");
        Err(ApiClientError::Other(Box::new(
            MultiNodeClientError::NoNodesFound,
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
                        MultiNodeClientError::NodeTimedOut {
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
                            MultiNodeClientError::UnhealthyNode { node_id },
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

    let (node_id, client, latency) = fastest_client.ok_or(ApiClientError::new(
        ApiEndpoint::HealthCheck,
        MultiNodeClientError::NoResponsiveNodesFound {
            latency: timeout.as_millis() as u64,
        },
    ))?;

    tracing::info!("chosen node is {} with latency {}", node_id, latency);

    Ok(client)
}

/* MiddlewareBuilder implementation for MultiNodeClient */

#[derive(Default)]
pub struct MultiNodeClientBuilder {
    gateway_client: Option<GrpcClient>,
    timeout: Option<Duration>,
    node_client_template: Option<xmtp_api_grpc::client::ClientBuilder>,
}

/// MultiNodeClientError is used to wrap the errors from the multi node client.
#[derive(Debug, Error)]
pub enum MultiNodeClientBuilderError {
    #[error("timeout must be greater than 0")]
    InvalidTimeout,
    #[error("gateway client is required")]
    MissingGatewayClient,
    #[error("node template is required")]
    MissingNodeTemplate,
}

impl MiddlewareBuilder for MultiNodeClientBuilder {
    type Output = MultiNodeClient;
    type Error = MultiNodeClientBuilderError;

    fn set_gateway_client(&mut self, gateway_client: GrpcClient) -> Result<(), Self::Error> {
        self.gateway_client = Some(gateway_client);
        Ok(())
    }

    fn set_timeout(&mut self, timeout: Duration) -> Result<(), Self::Error> {
        self.timeout = Some(timeout);
        Ok(())
    }

    fn set_client_builder_template(
        &mut self,
        template: xmtp_api_grpc::client::ClientBuilder,
    ) -> Result<(), Self::Error> {
        self.node_client_template = Some(template);
        Ok(())
    }

    fn build(self) -> Result<Self::Output, Self::Error> {
        let gateway_client = self
            .gateway_client
            .ok_or(MultiNodeClientBuilderError::MissingGatewayClient)?;

        let template = self
            .node_client_template
            .ok_or(MultiNodeClientBuilderError::MissingNodeTemplate)?;

        let timeout = self
            .timeout
            .ok_or(MultiNodeClientBuilderError::InvalidTimeout)?;

        Ok(MultiNodeClient {
            gateway_client,
            inner: OnceCell::new(),
            timeout,
            node_client_template: template,
        })
    }
}

/* MiddlewareBuilder */

pub trait MiddlewareBuilder {
    type Output;
    type Error;

    /// set the gateway client for node discovery
    fn set_gateway_client(&mut self, gateway_client: GrpcClient) -> Result<(), Self::Error>;

    /// max timeout allowed for nodes to respond
    fn set_timeout(&mut self, timeout: Duration) -> Result<(), Self::Error>;

    /// set the client builder template used to construct discovered node clients
    fn set_client_builder_template(
        &mut self,
        template: xmtp_api_grpc::client::ClientBuilder,
    ) -> Result<(), Self::Error>;

    fn build(self) -> Result<Self::Output, Self::Error>;
}

pub mod multinode {
    use super::*;

    pub fn builder() -> MultiNodeClientBuilder {
        MultiNodeClientBuilder::default()
    }
}
