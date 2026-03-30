mod api_stats;
mod boxed_streams;
mod builder;
mod client_bundle;
mod combinators;
mod combined;
mod d14n;
pub mod stream;
mod v3;

pub use api_stats::*;
pub use boxed_streams::*;
pub use builder::*;
pub use client_bundle::*;
pub use combinators::*;
pub use combined::*;
pub use d14n::*;
pub use stream::*;
pub use v3::*;

use std::collections::HashMap;
use xmtp_common::{RetryableError, retryable};
use xmtp_proto::{
    ConversionError,
    api::{self, ApiClientError, BodyError, Client, Query},
    prelude::{ApiBuilder, NetConnectConfig},
};

/// Build per-node gRPC clients by calling GetNodes and constructing a client for each node URL.
/// Shared between D14nClient and MigrationClient.
///
/// Accepts an optional `ClientBuilder` template so per-node clients inherit `app_version`
/// and `libxmtp_version` metadata from the parent client. When `None`, a fresh builder is used.
pub(crate) async fn build_node_clients(
    gateway: &impl Client,
    template: Option<&xmtp_api_grpc::ClientBuilder>,
) -> Result<HashMap<u32, Box<dyn Client + Send + Sync>>, ApiClientError> {
    use crate::d14n::GetNodes;
    use xmtp_api_grpc::GrpcClient;

    let response = api::retry(GetNodes::builder().build()?)
        .query(gateway)
        .await?;

    let mut clients: HashMap<u32, Box<dyn Client + Send + Sync>> = HashMap::new();
    for (node_id, url) in response.nodes {
        let mut builder = template.cloned().unwrap_or_else(GrpcClient::builder);
        match url.parse() {
            Ok(host) => {
                builder.set_host(host);
                match builder.build() {
                    Ok(client) => {
                        clients.insert(node_id, Box::new(client));
                    }
                    Err(e) => {
                        tracing::warn!(node_id, %url, error = %e, "failed to build grpc client for node");
                    }
                }
            }
            Err(e) => {
                tracing::warn!(node_id, %url, error = %e, "failed to parse url for node");
            }
        }
    }
    Ok(clients)
}

#[derive(thiserror::Error, Debug)]
pub enum QueryError {
    #[error(transparent)]
    ApiClient(#[from] ApiClientError),
    #[error(transparent)]
    Envelope(#[from] crate::protocol::EnvelopeError),
    #[error(transparent)]
    Conversion(#[from] ConversionError),
    #[error(transparent)]
    Body(#[from] BodyError),
}

impl From<crate::protocol::EnvelopeError> for ApiClientError {
    fn from(e: crate::protocol::EnvelopeError) -> ApiClientError {
        ApiClientError::Other(Box::new(e))
    }
}

impl RetryableError for QueryError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::ApiClient(c) => retryable!(c),
            Self::Envelope(_e) => false,
            Self::Conversion(_c) => false,
            Self::Body(b) => retryable!(b),
        }
    }
}
