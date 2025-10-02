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
    inner: C,
}

impl AggregateClient<GrpcClient> {
    pub async fn new(
        gateway_client: &GrpcClient,
        timeout: Duration,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let nodes = get_nodes(gateway_client).await?;
        let selected = get_fastest_node(nodes, timeout).await?;

        Ok(Self { inner: selected })
    }

    pub async fn refresh(
        &mut self,
        timeout: Duration,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let nodes = get_nodes(&self.inner).await?;
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

async fn get_nodes(
    gateway_client: &GrpcClient,
) -> Result<HashMap<u32, GrpcClient>, Box<dyn std::error::Error + Send + Sync>> {
    let endpoint = GetNodes::builder().build()?;
    let response = endpoint.query(gateway_client).await?;

    let mut clients = HashMap::new();
    for (node_id, url) in response.nodes {
        let mut client_builder = GrpcClient::builder();

        client_builder.set_tls(url.starts_with("https://"));
        client_builder.set_host(url);

        let client = client_builder.build().await?;
        clients.insert(node_id, client);
    }

    Ok(clients)
}

async fn get_fastest_node(
    clients: HashMap<u32, GrpcClient>,
    timeout: Duration,
) -> Result<GrpcClient, Box<dyn std::error::Error + Send + Sync>> {
    let futures = clients.into_iter().map(|(node_id, client)| async move {
        let start = Instant::now();
        let endpoint = HealthCheck::builder().build().ok()?;
        let result = xmtp_common::time::timeout(timeout, endpoint.query(&client)).await;

        match result {
            Ok(Ok(_)) => Some((node_id, client, start.elapsed().as_millis() as u64)),
            _ => None,
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
