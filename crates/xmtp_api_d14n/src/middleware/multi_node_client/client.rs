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
    // The gateway, always: `host()` is a connection identity and must not
    // change over the client's lifetime, while the resolved node is a
    // transient the gateway can re-issue. The gateway URL is the stable
    // name for "this multi-node backend".
    fn host(&self) -> &str {
        self.gateway_client.host()
    }

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

    async fn bidi_stream(
        &self,
        request: http::request::Builder,
        path: http::uri::PathAndQuery,
        body: xmtp_common::BoxDynStream<'static, Bytes>,
    ) -> Result<http::Response<BytesStream>, ApiClientError> {
        let inner = self.init_inner().await?;

        inner.bidi_stream(request, path, body).await
    }
}

#[xmtp_common::async_trait]
impl<T: IsConnectedCheck> IsConnectedCheck for MultiNodeClient<T> {
    async fn is_connected(&self) -> bool {
        self.gateway_client.is_connected().await
    }
}
