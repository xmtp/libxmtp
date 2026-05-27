use crate::{
    MultiNodeClientError,
    d14n::{GetNodes, HealthCheck},
};
use futures::StreamExt;
use std::collections::HashMap;
use xmtp_api_grpc::{ClientBuilder, GrpcClient};
use xmtp_common::time::{Duration, Instant};
use xmtp_proto::api::{self, Client, Query};
use xmtp_proto::prelude::{ApiBuilder, NetConnectConfig};
use xmtp_proto::{ApiEndpoint, api::ApiClientError};

/// Get the nodes from the gateway server and build the clients for each node.
pub(super) async fn get_nodes<C: Client>(
    gateway_client: &C,
    template: &ClientBuilder,
) -> Result<HashMap<u32, GrpcClient>, ApiClientError> {
    let response = api::retry(GetNodes::builder().build()?)
        .query(gateway_client)
        .await
        .map_err(|e| {
            tracing::error!("failed to get nodes from gateway: {}", e);
            e.endpoint(ApiEndpoint::GetNodes)
        })?;

    let max_concurrency = if response.nodes.is_empty() {
        tracing::warn!("no nodes found");
        Err(ApiClientError::from(MultiNodeClientError::NoNodesFound))
    } else {
        Ok(response.nodes.len())
    }?;

    tracing::info!("got nodes from gateway: {:?}", response.nodes);

    let mut clients: HashMap<u32, GrpcClient> = HashMap::new();

    let mut stream =
        futures::stream::iter(response.nodes.into_iter().map(|(node_id, url)| async move {
            // Clone a fresh builder per node so we can mutate it safely.
            let mut client_builder = template.clone();

            tracing::debug!("building client for node {}: {}", node_id, url);

            client_builder.set_host(
                url.parse()
                    .map_err(|e| (node_id, MultiNodeClientError::from(e)))?,
            );

            let client = client_builder.build().map_err(|e| (node_id, e.into()))?;

            Ok::<_, (u32, MultiNodeClientError)>((node_id, client, url))
        }))
        .buffer_unordered(max_concurrency);

    while let Some(res) = stream.next().await {
        match res {
            Ok((node_id, client, url)) => {
                tracing::info!("built client for node {}: {}", node_id, url);
                clients.insert(node_id, client);
            }
            Err(err) => {
                tracing::error!("failed to build client for node {}: {}", err.0, err.1);
            }
        }
    }

    if clients.is_empty() {
        tracing::error!("all node clients failed to build");
        return Err(MultiNodeClientError::AllNodeClientsFailedToBuild.into());
    }

    tracing::debug!("built clients for nodes: {:?}", clients.keys());

    Ok(clients)
}

/// Get the fastest node from the list of endpoints.
pub async fn get_fastest_node(
    clients: HashMap<u32, GrpcClient>,
    timeout: Duration,
) -> Result<GrpcClient, ApiClientError> {
    let endpoint = HealthCheck::builder().build().map_err(|e| {
        tracing::error!("failed to build healthcheck endpoint: {}", e);
        MultiNodeClientError::BodyError(e)
    })?;

    let max_concurrency = if clients.is_empty() {
        tracing::warn!("no nodes found");
        Err(ApiClientError::other(MultiNodeClientError::NoNodesFound))
    } else {
        Ok(clients.len())
    }?;

    let mut fastest_client: Option<(u32, GrpcClient, u64)> = None;
    let mut failed_nodes = Vec::new();

    let mut stream = futures::stream::iter(clients.into_iter().map(|(node_id, client)| {
        let mut endpoint = endpoint.clone();

        async move {
            tracing::debug!("healthcheck node {}", node_id);

            let start = Instant::now();

            xmtp_common::time::timeout(timeout, endpoint.query(&client))
                .await
                .map_err(|_| {
                    tracing::error!("node {} timed out after {}ms", node_id, timeout.as_millis());
                    MultiNodeClientError::NodeTimedOut {
                        node_id,
                        latency: timeout.as_millis() as u64,
                    }
                    .into()
                })
                .and_then(|r| {
                    r.map_err(|e| {
                        tracing::error!("node {} is unhealthy: {}", node_id, e);
                        e.endpoint(ApiEndpoint::HealthCheck)
                    })
                })
                .map(|_| (node_id, client, start.elapsed().as_millis() as u64))
        }
    }))
    .buffer_unordered(max_concurrency);

    while let Some(res) = stream.next().await {
        match res {
            Ok((node_id, client, latency)) => {
                tracing::debug!("node {} has latency {}ms", node_id, latency);
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
                failed_nodes.push(e);
            }
        }
    }

    let (node_id, client, latency) = fastest_client.ok_or_else(|| {
        tracing::error!(
            "no responsive nodes found, {} node(s) failed health checks",
            failed_nodes.len()
        );
        ApiClientError::from(MultiNodeClientError::NoResponsiveNodesFound {
            latency: timeout.as_millis() as u64,
        })
    })?;

    tracing::info!("chosen node is {} with latency {}ms", node_id, latency);

    Ok(client)
}

#[cfg(test)]
mod tests {
    use super::*;
    use prost::Message;
    use prost::bytes::Bytes;
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use xmtp_api_grpc::GrpcClient;
    use xmtp_proto::api::ApiClientError;
    use xmtp_proto::api::mock::{MockError, MockNetworkClient};
    use xmtp_proto::xmtp::xmtpv4::payer_api::GetNodesResponse;

    fn encoded_nodes_response() -> Bytes {
        let mut nodes = HashMap::new();
        nodes.insert(1u32, "http://localhost:65535".to_string());
        Bytes::from(GetNodesResponse { nodes }.encode_to_vec())
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn retry_get_nodes_recovers_from_transient_failure() {
        let counter = Arc::new(AtomicUsize::new(0));
        let response = encoded_nodes_response();

        let mut mock = MockNetworkClient::new();
        mock.expect_request().returning(move |_, _, _| {
            let n = counter.fetch_add(1, Ordering::SeqCst);
            if n == 0 {
                Err(ApiClientError::client(MockError::ARetryableError))
            } else {
                Ok(http::Response::new(response.clone()))
            }
        });

        let clients = get_nodes(&mock, &GrpcClient::builder())
            .await
            .expect("get_nodes should recover from a single transient failure");
        assert_eq!(clients.len(), 1, "expected one built client");
        assert!(clients.contains_key(&1), "expected node id 1 in clients");
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn retry_get_nodes_exhausts_budget_on_repeated_transient_failure() {
        let mut mock = MockNetworkClient::new();
        mock.expect_request()
            .returning(|_, _, _| Err(ApiClientError::client(MockError::ARetryableError)));

        let err = get_nodes(&mock, &GrpcClient::builder())
            .await
            .expect_err("get_nodes should give up after exhausting the retry budget");

        // The gRPC path is attached by the query layer before get_nodes can re-tag it,
        // so the error contains the gRPC path rather than the ApiEndpoint display name.
        let expected_tag = "/xmtp.xmtpv4.payer_api.PayerApi/GetNodes";
        let err_string = err.to_string();
        assert!(
            err_string.contains(expected_tag),
            "expected endpoint tag {:?} in error, got: {}",
            expected_tag,
            err_string,
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn retry_get_nodes_fails_fast_on_non_retryable_error() {
        let mut mock = MockNetworkClient::new();
        mock.expect_request()
            .times(1)
            .returning(|_, _, _| Err(ApiClientError::client(MockError::ANonRetryableError)));

        let err = get_nodes(&mock, &GrpcClient::builder())
            .await
            .expect_err("get_nodes should not retry non-retryable errors");
        let _ = err; // assertion is enforced by mockall's `.times(1)` on Drop
    }
}
