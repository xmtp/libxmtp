use crate::middleware::multi_node_client::{errors::MultiNodeClientError, gateway_api::*};
use prost::bytes::Bytes;
use tokio::sync::OnceCell;
use xmtp_api_grpc::{ClientBuilder, GrpcClient, error::GrpcError};
use xmtp_common::time::Duration;
use xmtp_proto::api::{ApiClientError, Client};

/* MultiNodeClient struct and its implementations */

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
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::middleware::{MiddlewareBuilder, MultiNodeClientBuilder};
    use xmtp_configuration::GrpcUrls;
    use xmtp_proto::api_client::ApiBuilder;

    fn create_gateway_builder() -> ClientBuilder {
        let mut b = GrpcClient::builder();
        let url = url::Url::parse(GrpcUrls::GATEWAY).expect("valid gateway url");
        b.set_host(GrpcUrls::GATEWAY.to_string());
        b.set_tls(url.scheme() == "https");
        b
    }

    fn make_template(tls: bool) -> xmtp_api_grpc::ClientBuilder {
        let mut t = GrpcClient::builder();
        t.set_tls(tls);
        // host will be overridden per node
        t.set_host("http://placeholder".to_string());
        t
    }

    #[tokio::test]
    async fn builder_ok_when_all_set() {
        let mut b = MultiNodeClientBuilder::default();
        b.set_gateway_builder(create_gateway_builder()).unwrap();
        b.set_timeout(Duration::from_millis(100)).unwrap();
        let client = <MultiNodeClientBuilder as MiddlewareBuilder>::build(b).expect("build ok");
        let _ = client; // not used further
    }

    #[test]
    fn tls_guard_accepts_matching_https_tls_true() {
        let t = make_template(true);
        validate_tls_guard(&t, "https://example.com:443").expect("should accept");
    }

    #[test]
    fn tls_guard_accepts_matching_http_tls_false() {
        let t = make_template(false);
        validate_tls_guard(&t, "http://example.com:80").expect("should accept");
    }

    #[test]
    fn tls_guard_rejects_https_with_plain_template() {
        let t = make_template(false);
        let err = validate_tls_guard(&t, "https://example.com:443")
            .err()
            .unwrap();
        let msg = format!("{err}");
        assert!(msg.contains("tls channel"));
    }

    #[test]
    fn tls_guard_rejects_http_with_tls_template() {
        let t = make_template(true);
        let err = validate_tls_guard(&t, "http://example.com:80")
            .err()
            .unwrap();
        let msg = format!("{err}");
        assert!(msg.contains("tls channel"));
    }

    /// This test also serves as an example of how to use the MultiNodeClientBuilder and D14nClientBuilder.
    #[tokio::test]
    async fn d14n_builder_works_with_multinode() {
        use crate::D14nClientBuilder;
        use xmtp_proto::prelude::ApiBuilder;

        // 1) Create gateway builder.
        let mut gateway_builder = GrpcClient::builder();
        let url = url::Url::parse(GrpcUrls::GATEWAY).expect("valid gateway url");
        match url.scheme() {
            "https" => gateway_builder.set_tls(true),
            _ => gateway_builder.set_tls(false),
        }
        gateway_builder.set_host(GrpcUrls::GATEWAY.into());

        // 2) Configure multi-node builder with the gateway builder.
        let mut multi_node_builder = MultiNodeClientBuilder::default();

        // Multi-node specific configuration.
        // Pass the gateway builder to the multi-node builder.
        multi_node_builder
            .set_gateway_builder(gateway_builder.clone())
            .expect("gateway set on multi-node");

        // Multi-node specific configuration.
        // Set the timeout, used in multi-node client requests to the gateway.
        multi_node_builder
            .set_timeout(xmtp_common::time::Duration::from_millis(100))
            .unwrap();

        // ApiBuilder methods forward configuration to the node client template.
        // All GrpcClient instances will inherit these settings.
        multi_node_builder.set_tls(url.scheme() == "https");

        // All ApiBuilder methods are available:
        // multi_node_builder.set_libxmtp_version("1.0.0".into())?;
        // multi_node_builder.set_retry(Retry::default());

        // 3) Build D14n client with both builders
        // D14nClientBuilder.build() will call both builders' build() methods!
        let _d14n = D14nClientBuilder::new(multi_node_builder, gateway_builder);
    }
}
