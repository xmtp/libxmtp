use crate::d14n::{GetNodes, HealthCheck};
use futures::StreamExt;
use prost::bytes::Bytes;
use std::collections::HashMap;
use thiserror::Error;
use tokio::sync::OnceCell;
use xmtp_api_grpc::{
    ClientBuilder, GrpcClient,
    error::{GrpcBuilderError, GrpcError},
};
use xmtp_common::{
    RetryableError,
    time::{Duration, Instant},
};
use xmtp_configuration::internal_to_localhost;
use xmtp_proto::{
    ApiEndpoint,
    api::{ApiClientError, BodyError, Client, Query},
    prelude::ApiBuilder,
};
use xmtp_proto::{api::IsConnectedCheck, types::AppVersion};

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
    node_client_template: ClientBuilder,
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

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl IsConnectedCheck for MultiNodeClient {
    async fn is_connected(&self) -> bool {
        self.gateway_client.is_connected().await
    }
}

/// Get the nodes from the gateway server.
async fn get_nodes(
    gateway_client: &GrpcClient,
    template: &ClientBuilder,
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
            // Clone a fresh builder per node so we can mutate it safely.
            let mut client_builder = template.clone();
            tracing::debug!("building client for node {}: {}", node_id, url);
            // Validate TLS policy against the fully-qualified URL.
            validate_tls_guard(&client_builder, &url).map_err(|e| (node_id, e))?;
            let url = if cfg!(feature = "test-utils") || cfg!(test) {
                internal_to_localhost(&url)
            } else {
                url
            };
            tracing::debug!("changed url to {url}");
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
                    tracing::info!("{:?}", r);
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

/// Validate that the template's TLS configuration matches the URL scheme.
fn validate_tls_guard(
    template: &ClientBuilder,
    url: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let url_is_tls = url
        .parse::<url::Url>()
        .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?
        .scheme()
        == "https";

    (template.tls_channel == url_is_tls)
        .then_some(())
        .ok_or_else(|| -> Box<dyn std::error::Error + Send + Sync> {
            Box::new(MultiNodeClientError::TlsChannelMismatch {
                url_is_tls,
                client_builder_tls_channel: template.tls_channel,
            })
        })
}

/* MiddlewareBuilder trait */

pub trait MiddlewareBuilder {
    type Output;
    type Error;

    /// set the gateway client for node discovery
    fn set_gateway_client(&mut self, gateway_client: GrpcClient) -> Result<(), Self::Error>;

    /// max timeout allowed for nodes to respond
    fn set_timeout(&mut self, timeout: Duration) -> Result<(), Self::Error>;

    fn build(self) -> Result<Self::Output, Self::Error>;
}

pub mod multi_node {
    use super::*;

    pub fn builder() -> MultiNodeClientBuilder {
        MultiNodeClientBuilder::default()
    }
}

/* MiddlewareBuilder implementation for MultiNodeClient */

pub struct MultiNodeClientBuilder {
    gateway_client: Option<GrpcClient>,
    timeout: Duration,
    node_client_template: ClientBuilder,
}

impl Default for MultiNodeClientBuilder {
    fn default() -> Self {
        Self {
            gateway_client: None,
            timeout: Duration::from_millis(100),
            node_client_template: GrpcClient::builder(),
        }
    }
}

/// MultiNodeClientError is used to wrap the errors from the multi node client.
#[derive(Debug, Error)]
pub enum MultiNodeClientBuilderError {
    #[error(transparent)]
    GrpcBuilderError(#[from] GrpcBuilderError),
    #[error("timeout must be greater than 0")]
    InvalidTimeout,
    #[error("gateway client is required")]
    MissingGatewayClient,
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use xmtp_configuration::GrpcUrls;

    fn create_gateway() -> GrpcClient {
        let mut b = GrpcClient::builder();
        let url = url::Url::parse(GrpcUrls::GATEWAY).expect("valid gateway url");
        b.set_host(GrpcUrls::GATEWAY.to_string());
        b.set_tls(url.scheme() == "https");
        b.build().expect("gateway client")
    }

    fn make_template(tls: bool) -> xmtp_api_grpc::ClientBuilder {
        let mut t = GrpcClient::builder();
        t.set_tls(tls);
        // host will be overridden per node
        t.set_host("http://placeholder".to_string());
        t
    }

    #[tokio::test]
    async fn builder_ok_when_all_set() {
        let mut b = MultiNodeClientBuilder::default();
        b.set_gateway_client(create_gateway()).unwrap();
        b.set_timeout(Duration::from_millis(100)).unwrap();
        let client = <MultiNodeClientBuilder as MiddlewareBuilder>::build(b).expect("build ok");
        let _ = client; // not used further
    }

    #[test]
    fn tls_guard_accepts_matching_https_tls_true() {
        let t = make_template(true);
        validate_tls_guard(&t, "https://example.com:443").expect("should accept");
    }

    #[test]
    fn tls_guard_accepts_matching_http_tls_false() {
        let t = make_template(false);
        validate_tls_guard(&t, "http://example.com:80").expect("should accept");
    }

    #[test]
    fn tls_guard_rejects_https_with_plain_template() {
        let t = make_template(false);
        let err = validate_tls_guard(&t, "https://example.com:443")
            .err()
            .unwrap();
        let msg = format!("{err}");
        assert!(msg.contains("tls channel"));
    }

    #[test]
    fn tls_guard_rejects_http_with_tls_template() {
        let t = make_template(true);
        let err = validate_tls_guard(&t, "http://example.com:80")
            .err()
            .unwrap();
        let msg = format!("{err}");
        assert!(msg.contains("tls channel"));
    }

    #[tokio::test]
    async fn d14n_builder_works_with_multinode() {
        use crate::D14nClientBuilder;
        use xmtp_proto::prelude::ApiBuilder;

        // Prepare gateway builder.
        let mut gateway = GrpcClient::builder();
        let url = url::Url::parse(GrpcUrls::GATEWAY).expect("valid gateway url");
        match url.scheme() {
            "https" => gateway.set_tls(true),
            _ => gateway.set_tls(false),
        }
        gateway.set_host(GrpcUrls::GATEWAY.into());

        // Build the gateway client.
        let built_gateway = <xmtp_api_grpc::ClientBuilder as ApiBuilder>::build(gateway)
            .expect("gateway client built");

        // Configure multi-node builder via ApiBuilder methods and inject gateway
        let mut multi_node = MultiNodeClientBuilder::default();
        multi_node
            .set_timeout(xmtp_common::time::Duration::from_millis(100))
            .unwrap();
        // Ensure node template inherits TLS policy
        ClientBuilder::set_tls(
            &mut multi_node.node_client_template,
            url.scheme() == "https",
        );
        multi_node
            .set_gateway_client(built_gateway)
            .expect("gateway set on multi-node");

        // Build D14n client using multi-node as the message builder and gateway builder
        // Recreate a gateway builder for D14n builder (callers will normally pass the original builder)
        let mut gateway_b = GrpcClient::builder();
        gateway_b.set_host(GrpcUrls::GATEWAY.into());
        gateway_b.set_tls(url.scheme() == "https");
        let _d14n = D14nClientBuilder::new_stateless(multi_node, gateway_b);
    }
}
