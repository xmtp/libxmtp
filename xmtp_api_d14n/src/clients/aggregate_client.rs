use crate::d14n::{GetNodes, HealthCheck};
use futures::future::join_all;
use prost::bytes::Bytes;
use std::collections::HashMap;
use xmtp_api_grpc::GrpcClient;
use xmtp_common::time::{Duration, Instant};
use xmtp_proto::{
    prelude::ApiBuilder,
    traits::{ApiClientError, Client, Query},
};

pub struct AggregateClient<C>
where
    C: Client + Sync + Send,
{
    gateway_client: C,
    inner: C,
}

impl AggregateClient<GrpcClient> {
    pub async fn new(
        gateway_client: GrpcClient,
        timeout: Duration,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let nodes = get_nodes(&gateway_client).await?;
        let selected = get_fastest_node(nodes, timeout).await?;

        Ok(Self {
            gateway_client,
            inner: selected,
        })
    }

    pub async fn refresh(
        &mut self,
        timeout: Duration,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let nodes = get_nodes(&self.gateway_client).await?;
        let selected = get_fastest_node(nodes, timeout).await?;

        self.inner = selected;

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
) -> Result<HashMap<u32, GrpcClient>, Box<dyn std::error::Error + Send + Sync>> {
    let endpoint = GetNodes::builder()
        .build()
        .map_err(|e| format!("get nodes build failed: {e}"))?;

    let response = endpoint.query(gateway_client).await?;

    let futures = response.nodes.into_iter().map(|(node_id, url)| async move {
        let mut client_builder = GrpcClient::builder();
        let is_tls = url.starts_with("https://");

        client_builder.set_tls(is_tls);
        client_builder.set_host(url);

        let client = client_builder.build().await?;

        Ok::<_, Box<dyn std::error::Error + Send + Sync>>((node_id, client))
    });

    let results = join_all(futures).await;

    let mut clients = HashMap::new();
    for result in results {
        let (node_id, client) = result?;
        clients.insert(node_id, client);
    }

    Ok(clients)
}

async fn get_fastest_node(
    clients: HashMap<u32, GrpcClient>,
    timeout: Duration,
) -> Result<GrpcClient, Box<dyn std::error::Error + Send + Sync>> {
    let endpoint = HealthCheck::builder()
        .build()
        .map_err(|e| format!("get health check build failed: {e}"))?;

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
        .ok_or("No responsive nodes found")?;

    Ok(fastest_node.1)
}
