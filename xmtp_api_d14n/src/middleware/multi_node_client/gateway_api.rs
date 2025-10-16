use crate::{
    MultiNodeClientError,
    d14n::{GetNodes, HealthCheck},
};
use futures::StreamExt;
use std::collections::HashMap;
use xmtp_api_grpc::{ClientBuilder, GrpcClient};
use xmtp_common::time::{Duration, Instant};
use xmtp_proto::api::Query;
use xmtp_proto::api_client::ApiBuilder;
use xmtp_proto::{ApiEndpoint, api::ApiClientError};

/* Gateway API functions */

/// Get the nodes from the gateway server and build the clients for each one.
pub async fn get_nodes(
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
pub async fn get_fastest_node(
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

/// Validate that the template's TLS configuration matches the URL scheme.
pub fn validate_tls_guard(
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
