use crate::d14n::GetNodes;
use prost::bytes::Bytes;
use xmtp_proto::{
    traits::{Client, Query},
    traits::ApiClientError,
    prelude::ApiBuilder,
};
use xmtp_api_grpc::{
    {GrpcClient, GrpcStream},
    error::GrpcError,
};


pub struct AggregateClient<T>
where
    T: Client + Sync + Send,
{
    // Do we want to store node_id : client map?
    nodes: Vec<T>,
    selected: T,
}

impl AggregateClient<GrpcClient> {
    pub async fn new(gateway_client: &GrpcClient) -> Self {
        let nodes = Self::build_inner_clients(gateway_client).await.unwrap();

        // TODO: Select best
        let selected = nodes.first().unwrap().clone();

        Self {
            nodes,
            selected,
        }
    }

    pub async fn build_inner_clients(gateway_client: &GrpcClient) -> Result<Vec<GrpcClient>, Box<dyn std::error::Error + Send + Sync>> {
        // TODO: Cleanup unwrap
        let endpoint = GetNodes::builder().build().unwrap();
        let response = endpoint.query(gateway_client).await?;
        let mut inner_clients = Vec::new();

        for node_url in response.nodes {
            // Save latency somewhere
            let mut client_builder = GrpcClient::builder();
            let use_tls = node_url.starts_with("https://");

            client_builder.set_tls(use_tls);
            client_builder.set_host(node_url);
            
            let client = client_builder.build().await?;
            inner_clients.push(client);
        }

        Ok(inner_clients)
    }

    pub fn get_inner_client(&self) -> &GrpcClient {
        &self.selected
    }

    pub fn set_inner_client(&mut self, client: &GrpcClient) {
        self.selected = client.clone();
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Client for AggregateClient<GrpcClient> {
    type Error = GrpcError;
    type Stream = GrpcStream;

    async fn request(
        &self,
        request: http::request::Builder,
        path: http::uri::PathAndQuery,
        body: Bytes,
    ) -> Result<http::Response<Bytes>, ApiClientError<Self::Error>> {
        self.selected.request(request, path, body).await
    }

    async fn stream(
        &self,
        request: http::request::Builder,
        path: http::uri::PathAndQuery,
        body: Bytes,
    ) -> Result<http::Response<Self::Stream>, ApiClientError<Self::Error>> {
        self.selected.stream(request, path, body).await
    }
}