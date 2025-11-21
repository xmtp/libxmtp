use crate::middleware::multi_node_client::{errors::MultiNodeClientError, gateway_api::*};
use prost::bytes::Bytes;
use tokio::sync::OnceCell;
use xmtp_api_grpc::{ClientBuilder, GrpcClient, error::GrpcError};
use xmtp_common::time::Duration;
use xmtp_proto::api::{ApiClientError, Client, IsConnectedCheck};

/* MultiNodeClient struct and its implementations */

#[derive(Clone)]
pub struct MultiNodeClient {
    pub gateway_client: GrpcClient,
    pub inner: OnceCell<GrpcClient>,
    pub timeout: Duration,
    pub node_client_template: ClientBuilder,
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
#[xmtp_common::async_trait]
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

#[xmtp_common::async_trait]
impl IsConnectedCheck for MultiNodeClient {
    async fn is_connected(&self) -> bool {
        self.gateway_client.is_connected().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ReadWriteClient,
        middleware::{MiddlewareBuilder, MultiNodeClientBuilder},
        protocol::{InMemoryCursorStore, NoCursorStore},
        queries::D14nClient,
    };
    use std::sync::Arc;
    use xmtp_configuration::{GrpcUrls, PAYER_WRITE_FILTER};
    use xmtp_proto::api::Query;
    use xmtp_proto::api_client::{ApiBuilder, NetConnectConfig};
    use xmtp_proto::prelude::XmtpMlsClient;
    use xmtp_proto::types::GroupId;

    fn is_tls_enabled() -> bool {
        url::Url::parse(GrpcUrls::GATEWAY)
            .expect("valid gateway url")
            .scheme()
            == "https"
    }

    fn create_in_memory_cursor_store() -> Arc<InMemoryCursorStore> {
        Arc::new(InMemoryCursorStore::default())
    }

    fn create_gateway_builder() -> ClientBuilder {
        let mut gateway_builder = GrpcClient::builder();
        gateway_builder.set_host(GrpcUrls::GATEWAY.to_string());
        gateway_builder.set_tls(is_tls_enabled());
        gateway_builder
    }

    fn create_node_builder() -> ClientBuilder {
        let mut node_builder = GrpcClient::builder();
        node_builder.set_tls(is_tls_enabled());
        node_builder
    }

    fn create_multinode_client_builder() -> MultiNodeClientBuilder {
        let mut multi_node_builder = MultiNodeClientBuilder::default();
        multi_node_builder
            .set_gateway_builder(create_gateway_builder())
            .unwrap();
        multi_node_builder
            .set_node_client_builder(create_node_builder())
            .unwrap();
        multi_node_builder
            .set_timeout(Duration::from_millis(1000))
            .unwrap();
        multi_node_builder
    }

    fn create_multinode_client() -> MultiNodeClient {
        let multi_node_builder = create_multinode_client_builder();
        multi_node_builder.build().unwrap()
    }

    fn create_d14n_client()
    -> D14nClient<ReadWriteClient<MultiNodeClient, GrpcClient>, NoCursorStore> {
        let rw = ReadWriteClient::builder()
            .read(create_multinode_client_builder().build().unwrap())
            .write(create_gateway_builder().build().unwrap())
            .filter(PAYER_WRITE_FILTER)
            .build()
            .unwrap();
        D14nClient::new(rw, NoCursorStore).unwrap()
    }

    fn create_node_client_template(tls: bool) -> xmtp_api_grpc::ClientBuilder {
        let mut client_builder = GrpcClient::builder();
        client_builder.set_tls(tls);
        // host will be overridden per node
        client_builder.set_host("http://placeholder".to_string());
        client_builder
    }

    #[xmtp_common::test]
    fn tls_guard_accepts_matching_https_tls_true() {
        let t = create_node_client_template(true);
        validate_tls_guard(&t, "https://example.com:443").expect("should accept");
    }

    #[xmtp_common::test]
    fn tls_guard_accepts_matching_http_tls_false() {
        let t = create_node_client_template(false);
        validate_tls_guard(&t, "http://example.com:80").expect("should accept");
    }

    #[xmtp_common::test]
    fn tls_guard_rejects_https_with_plain_template() {
        let t = create_node_client_template(false);
        let err = validate_tls_guard(&t, "https://example.com:443")
            .err()
            .unwrap();
        let msg = format!("{err}");
        assert!(msg.contains("tls channel"));
    }

    #[xmtp_common::test]
    fn tls_guard_rejects_http_with_tls_template() {
        let t = create_node_client_template(true);
        let err = validate_tls_guard(&t, "http://example.com:80")
            .err()
            .unwrap();
        let msg = format!("{err}");
        assert!(msg.contains("tls channel"));
    }

    /// This test also serves as an example of how to use the MultiNodeClientBuilder and D14nClientBuilder.
    #[xmtp_common::test]
    async fn build_multinode_as_d14n() {
        use crate::D14nClient;
        use xmtp_proto::prelude::ApiBuilder;

        // 1) Create gateway builder.
        let gateway_builder = create_gateway_builder();
        let node_builder = create_node_builder();

        // 2) Configure multi-node builder with the gateway builder.
        let mut multi_node_builder = MultiNodeClientBuilder::default();

        // Multi-node specific configuration.
        // Pass the gateway builder to the multi-node builder.
        multi_node_builder
            .set_gateway_builder(gateway_builder.clone())
            .expect("gateway set on multi-node");

        multi_node_builder
            .set_node_client_builder(node_builder)
            .expect("node set on multi-node");

        // Multi-node specific configuration.
        // Set the timeout, used in multi-node client requests to the gateway.
        multi_node_builder
            .set_timeout(xmtp_common::time::Duration::from_millis(1000))
            .unwrap();

        // All ApiBuilder methods are available:
        // multi_node_builder.set_libxmtp_version("1.0.0".into())?;
        // multi_node_builder.set_retry(Retry::default());

        let cursor_store = create_in_memory_cursor_store();
        let multi_node_client = multi_node_builder.build().unwrap();
        let gateway_client = gateway_builder.build().unwrap();

        let rw = ReadWriteClient::builder()
            .read(multi_node_client)
            .write(gateway_client)
            .filter(PAYER_WRITE_FILTER)
            .build()
            .unwrap();
        // 3) Build D14n client with both clients
        let _d14n = D14nClient::new(rw, cursor_store).unwrap();
    }

    /// This test also serves as an example of how to use the MultiNodeClientBuilder standalone.
    #[xmtp_common::test]
    async fn build_multinode_as_standalone() {
        let gateway_builder = create_gateway_builder();
        let node_builder = create_node_builder();
        let mut multi_node_builder = MultiNodeClientBuilder::default();
        multi_node_builder
            .set_gateway_builder(gateway_builder.clone())
            .expect("gateway set on multi-node");

        multi_node_builder
            .set_node_client_builder(node_builder)
            .expect("node set on multi-node");

        multi_node_builder
            .set_timeout(xmtp_common::time::Duration::from_millis(100))
            .unwrap();

        let _ = multi_node_builder
            .build()
            .expect("failed to build multi-node client");
    }

    #[xmtp_common::test]
    async fn d14n_request_latest_group_message() {
        let client = create_d14n_client();
        let id: GroupId = GroupId::from(vec![]);
        let response = client.query_latest_group_message(id).await;
        match response {
            Err(e) => {
                let err_str = e.to_string();
                // The query shouldn't return a valid message.
                // But it shouldn't return any other type of error.
                assert!(err_str.contains("missing field group_message"));
            }
            Ok(_) => panic!("expected error for empty group id"),
        }
    }

    #[xmtp_common::test]
    async fn multinode_request_latest_group_message() {
        use crate::d14n::GetNewestEnvelopes;
        let client = create_multinode_client();
        let mut endpoint = GetNewestEnvelopes::builder().topic(vec![]).build().unwrap();
        let response = endpoint.query(&client).await.unwrap();
        assert!(!response.results.is_empty());
    }
}
