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
};

/// Build per-node gRPC clients by calling GetNodes and constructing a client for each node URL.
/// Shared between D14nClient and MigrationClient.
///
/// When `app_version` is provided, per-node clients carry the same version metadata as the parent.
pub(crate) async fn build_node_clients(
    gateway: &impl Client,
    app_version: Option<&xmtp_proto::types::AppVersion>,
) -> Result<HashMap<u32, xmtp_api_grpc::GrpcClient>, ApiClientError> {
    use crate::d14n::GetNodes;
    use xmtp_api_grpc::GrpcClient;
    use xmtp_proto::prelude::{ApiBuilder, NetConnectConfig};

    let response = api::retry(GetNodes::builder().build()?)
        .query(gateway)
        .await?;

    let clients = response
        .nodes
        .into_iter()
        .filter_map(|(node_id, url)| {
            let host = url
                .parse()
                .map_err(|e| {
                    tracing::warn!(node_id, %url, error = %e, "failed to parse url for node");
                })
                .ok()?;
            let client = match app_version {
                Some(v) => GrpcClient::create_with_version(host, v.clone()),
                None => {
                    let mut b = GrpcClient::builder();
                    b.set_host(host);
                    b.build()
                }
            }
            .map_err(|e| {
                tracing::warn!(node_id, %url, error = %e, "failed to build grpc client for node");
            })
            .ok()?;
            Some((node_id, client))
        })
        .collect();
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
