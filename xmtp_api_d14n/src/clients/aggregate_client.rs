use crate::d14n::{GetNodes, HealthCheck};
use futures::future::join_all;
use prost::bytes::Bytes;
use std::collections::HashMap;
use thiserror::Error;
use xmtp_api_grpc::{GrpcClient, error::GrpcError};
use xmtp_common::time::{Duration, Instant};
use xmtp_proto::{
    ApiEndpoint,
    prelude::ApiBuilder,
    traits::{ApiClientError, Client, Query},
};

pub struct AggregateClient<C>
where
    C: Client + Sync + Send,
{
    gateway_client: C,
    inner: C,
    timeout: Duration,
}

impl AggregateClient<GrpcClient> {
    pub async fn new(
        gateway_client: GrpcClient,
        timeout: Duration,
    ) -> Result<Self, AggregateClientError> {
        if timeout.as_millis() == 0 {
            return Err(AggregateClientError::InvalidTimeout);
        }

        let nodes = get_nodes(&gateway_client).await.map_err(AggregateClientError::from)?;
        let inner = get_fastest_node(nodes, timeout).await.map_err(AggregateClientError::from)?;

        Ok(Self {
            gateway_client,
            inner,
            timeout,
        })
    }

    /// refresh checks the fastest node and updates the inner client
    /// should only be called when there are no active requests or streams
    pub async fn refresh(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let nodes = get_nodes(&self.gateway_client).await?;
        self.inner = get_fastest_node(nodes, self.timeout).await?;
        Ok(())
    }
}

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

async fn get_nodes(
    gateway_client: &GrpcClient,
) -> Result<HashMap<u32, GrpcClient>, ApiClientError<GrpcError>> {
    let response = GetNodes::builder()
        .build()?
        .query(gateway_client)
        .await
        .map_err(|_| {
            ApiClientError::new(
                ApiEndpoint::GetNodes,
                AggregateClientError::NoNodesFound.into(),
            )
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
            AggregateClientError::AllNodeClientsFailedToBuild.into(),
        ));
    }

    Ok(clients)
}

async fn get_fastest_node(
    clients: HashMap<u32, GrpcClient>,
    timeout: Duration,
) -> Result<GrpcClient, ApiClientError<GrpcError>> {
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
            AggregateClientError::NoResponsiveNodesFound.into(),
        ))?;

    Ok(fastest_node.1)
}

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

impl From<AggregateClientError> for GrpcError {
    fn from(error: AggregateClientError) -> Self {
        GrpcError::NotFound(error.to_string())
    }
}
