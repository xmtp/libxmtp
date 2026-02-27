use crate::MultiNodeClientBuilderError;
use crate::middleware::multi_node_client::gateway_api::*;
use derive_builder::Builder;
use prost::bytes::Bytes;
use tokio::sync::OnceCell;
use xmtp_api_grpc::{ClientBuilder, GrpcClient};
use xmtp_common::time::Duration;
use xmtp_configuration::MULTI_NODE_TIMEOUT_MS;
use xmtp_proto::api::{ApiClientError, BytesStream, Client, IsConnectedCheck};

/* MultiNodeClient struct and its implementations */

#[derive(Clone, Default, Builder)]
#[builder(build_fn(validate = "Self::validate", error = "MultiNodeClientBuilderError"))]
pub struct MultiNodeClient<T> {
    pub gateway_client: T,
    #[builder(default)]
    pub inner: OnceCell<GrpcClient>,
    #[builder(default = Duration::from_millis(MULTI_NODE_TIMEOUT_MS))]
    pub timeout: Duration,
    pub node_client_template: ClientBuilder,
}

impl<T> MultiNodeClientBuilder<T> {
    fn validate(&self) -> Result<(), MultiNodeClientBuilderError> {
        if let Some(t) = self.timeout
            && t.is_zero()
        {
            return Err(MultiNodeClientBuilderError::InvalidTimeout);
        }
        Ok(())
    }
}

impl<T: Clone> MultiNodeClient<T> {
    pub fn builder() -> MultiNodeClientBuilder<T> {
        MultiNodeClientBuilder::default()
    }
}

// TODO: Future PR implements a refresh() method that updates the inner client.
// In order to do so we need to use an OnceCell<ArcSwap<GrpcClient>>, so that
// we can update swap the inner client inside an OnceCell.
impl<T: Client> MultiNodeClient<T> {
    async fn init_inner(&self) -> Result<&GrpcClient, ApiClientError> {
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
impl<T: Client> Client for MultiNodeClient<T> {
    async fn request(
        &self,
        request: http::request::Builder,
        path: http::uri::PathAndQuery,
        body: Bytes,
    ) -> Result<http::Response<Bytes>, ApiClientError> {
        let inner = self.init_inner().await?;

        inner.request(request, path, body).await
    }

    async fn stream(
        &self,
        request: http::request::Builder,
        path: http::uri::PathAndQuery,
        body: Bytes,
    ) -> Result<http::Response<BytesStream>, ApiClientError> {
        let inner = self.init_inner().await?;

        inner.stream(request, path, body).await
    }
}

#[xmtp_common::async_trait]
impl<T: IsConnectedCheck> IsConnectedCheck for MultiNodeClient<T> {
    async fn is_connected(&self) -> bool {
        self.gateway_client.is_connected().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::middleware::multi_node_client::client::MultiNodeClientBuilder;
    use crate::{
        ReadWriteClient,
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

    fn create_multinode_client_builder() -> MultiNodeClientBuilder<GrpcClient> {
        MultiNodeClient::builder()
            .gateway_client(create_gateway_builder().build().unwrap())
            .node_client_template(create_node_builder())
            .timeout(Duration::from_millis(1000))
            .clone()
    }

    fn create_multinode_client() -> MultiNodeClient<GrpcClient> {
        let multi_node_builder = create_multinode_client_builder();
        multi_node_builder.build().unwrap()
    }

    fn create_d14n_client()
    -> D14nClient<ReadWriteClient<MultiNodeClient<GrpcClient>, GrpcClient>, NoCursorStore> {
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
        let mut multi_node_builder = MultiNodeClient::builder();

        // Multi-node specific configuration.
        // Pass the gateway builder to the multi-node builder.
        multi_node_builder.gateway_client(gateway_builder.clone().build().unwrap());

        multi_node_builder.node_client_template(node_builder);

        // Multi-node specific configuration.
        // Set the timeout, used in multi-node client requests to the gateway.
        multi_node_builder.timeout(xmtp_common::time::Duration::from_millis(1000));

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
        let mut multi_node_builder = MultiNodeClient::builder();
        multi_node_builder.gateway_client(gateway_builder.clone().build().unwrap());

        multi_node_builder.node_client_template(node_builder);

        multi_node_builder.timeout(xmtp_common::time::Duration::from_millis(100));

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
