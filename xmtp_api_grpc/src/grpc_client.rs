use prost::bytes::Bytes;
use std::time::Duration;
use thiserror::Error;
use tonic::{
    metadata::{self, MetadataMap, MetadataValue},
    transport::{self, Channel, ClientTlsConfig},
};
use tracing::Instrument;
use xmtp_proto::{
    api_client::ApiBuilder,
    traits::{ApiClientError, Client},
};

use crate::GrpcError;

impl From<GrpcError> for ApiClientError<GrpcError> {
    fn from(source: GrpcError) -> ApiClientError<GrpcError> {
        ApiClientError::Client { source }
    }
}

#[derive(Clone)]
pub struct GrpcClient {
    inner: tonic::client::Grpc<Channel>,
    app_version: MetadataValue<metadata::Ascii>,
    libxmtp_version: MetadataValue<metadata::Ascii>,
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Client for GrpcClient {
    type Error = crate::GrpcError;
    type Stream = tonic::Streaming<Bytes>;

    async fn request<T>(
        &self,
        request: http::request::Builder,
        path: http::uri::PathAndQuery,
        body: Vec<u8>,
    ) -> Result<http::Response<T>, ApiClientError<Self::Error>>
    where
        Self: Sized,
        T: Default + prost::Message + 'static,
    {
        let client = &mut self.inner.clone();
        client
            .ready()
            .await
            .map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e),
                )
            })
            .map_err(GrpcError::from)?;

        let request = request.body(body)?;
        let (parts, body) = request.into_parts();
        let mut tonic_request = tonic::Request::from_parts(
            MetadataMap::from_headers(parts.headers),
            parts.extensions,
            body,
        );
        let metadata = tonic_request.metadata_mut();
        // must be lowercase otherwise panics
        metadata.append("x-app-version", self.app_version.clone());
        metadata.append("x-libxmtp-version", self.libxmtp_version.clone());
        let codec = tonic::codec::ProstCodec::default();

        let response = client
            .unary(tonic_request, path, codec)
            .await
            .map_err(GrpcError::from)?;

        let (metadata, body, extensions) = response.into_parts();
        let mut response = http::Response::new(body);
        *response.version_mut() = http::version::Version::HTTP_2;
        *response.headers_mut() = metadata.into_headers();
        *response.extensions_mut() = extensions;
        Ok(response)
    }

    async fn stream(
        &self,
        _request: http::request::Builder,
        _body: Vec<u8>,
    ) -> Result<http::Response<Self::Stream>, ApiClientError<Self::Error>> {
        // same as unary but server_streaming method
        todo!()
    }
}

impl GrpcClient {
    pub fn builder() -> ClientBuilder {
        ClientBuilder::default()
    }
}

#[derive(Default)]
pub struct ClientBuilder {
    host: Option<String>,
    /// version of the app
    app_version: Option<MetadataValue<metadata::Ascii>>,
    /// Version of the libxmtp core library
    libxmtp_version: Option<MetadataValue<metadata::Ascii>>,
    /// Whether or not the channel should use TLS
    tls_channel: bool,
}

#[derive(Debug, Error)]
pub enum GrpcBuilderError {
    #[error("app version required to create client")]
    MissingAppVersion,
    #[error("libxmtp core library version required to create client")]
    MissingLibxmtpVersion,
    #[error("host url required to create client")]
    MissingHostUrl,
    #[error("payer url required to create client")]
    MissingPayerUrl,
    #[error(transparent)]
    Metadata(#[from] metadata::errors::InvalidMetadataValue),
    #[error(transparent)]
    Transport(#[from] transport::Error),
    #[error("Invalid URI during channel creation")]
    InvalidUri(#[from] http::uri::InvalidUri),
}

impl ApiBuilder for ClientBuilder {
    type Output = GrpcClient;
    type Error = GrpcBuilderError;

    fn set_libxmtp_version(&mut self, version: String) -> Result<(), Self::Error> {
        self.libxmtp_version = Some(MetadataValue::try_from(&version)?);
        Ok(())
    }

    fn set_app_version(&mut self, version: String) -> Result<(), Self::Error> {
        self.app_version = Some(MetadataValue::try_from(&version)?);
        Ok(())
    }

    fn set_tls(&mut self, tls: bool) {
        self.tls_channel = tls;
    }

    fn set_host(&mut self, host: String) {
        self.host = Some(host);
    }

    async fn build(self) -> Result<Self::Output, Self::Error> {
        let host = self.host.ok_or(GrpcBuilderError::MissingHostUrl)?;
        let channel = match self.tls_channel {
            true => create_tls_channel(host).await?,
            false => Channel::from_shared(host)?.connect().await?,
        };

        Ok(GrpcClient {
            inner: tonic::client::Grpc::new(channel),
            app_version: self
                .app_version
                .unwrap_or(MetadataValue::try_from("0.0.0")?),
            libxmtp_version: self.libxmtp_version.unwrap_or(MetadataValue::try_from(
                env!("CARGO_PKG_VERSION").to_string(),
            )?),
        })
    }
}

#[tracing::instrument(level = "trace", skip_all)]
pub async fn create_tls_channel(address: String) -> Result<Channel, GrpcBuilderError> {
    let span = tracing::debug_span!("grpc_connect", address);
    let channel = Channel::from_shared(address)?
        // Purpose: This setting controls the size of the initial connection-level flow control window for HTTP/2, which is the underlying protocol for gRPC.
        // Functionality: Flow control in HTTP/2 manages how much data can be in flight on the network. Setting the initial connection window size to (1 << 31) - 1 (the maximum possible value for a 32-bit integer, which is 2,147,483,647 bytes) essentially allows the client to receive a very large amount of data from the server before needing to acknowledge receipt and permit more data to be sent. This can be particularly useful in high-latency networks or when transferring large amounts of data.
        // Impact: Increasing the window size can improve throughput by allowing more data to be in transit at a time, but it may also increase memory usage and can potentially lead to inefficient use of bandwidth if the network is unreliable.
        .initial_connection_window_size(Some((1 << 31) - 1))
        // Purpose: Configures whether the client should send keep-alive pings to the server when the connection is idle.
        // Functionality: When set to true, this option ensures that periodic pings are sent on an idle connection to keep it alive and detect if the server is still responsive.
        // Impact: This helps maintain active connections, particularly through NATs, load balancers, and other middleboxes that might drop idle connections. It helps ensure that the connection is promptly usable when new requests need to be sent.
        .keep_alive_while_idle(true)
        // Purpose: Sets the maximum amount of time the client will wait for a connection to be established.
        // Functionality: If a connection cannot be established within the specified duration, the attempt is aborted and an error is returned.
        // Impact: This setting prevents the client from waiting indefinitely for a connection to be established, which is crucial in scenarios where rapid failure detection is necessary to maintain responsiveness or to quickly fallback to alternative services or retry logic.
        .connect_timeout(Duration::from_secs(10))
        // Purpose: Configures the TCP keep-alive interval for the socket connection.
        // Functionality: This setting tells the operating system to send TCP keep-alive probes periodically when no data has been transferred over the connection within the specified interval.
        // Impact: Similar to the gRPC-level keep-alive, this helps keep the connection alive at the TCP layer and detect broken connections. It's particularly useful for detecting half-open connections and ensuring that resources are not wasted on unresponsive peers.
        .tcp_keepalive(Some(Duration::from_secs(15)))
        // Purpose: Sets a maximum duration for the client to wait for a response to a request.
        // Functionality: If a response is not received within the specified timeout, the request is canceled and an error is returned.
        // Impact: This is critical for bounding the wait time for operations, which can enhance the predictability and reliability of client interactions by avoiding indefinitely hanging requests.
        .timeout(Duration::from_secs(120))
        // Purpose: Specifies how long the client will wait for a response to a keep-alive ping before considering the connection dead.
        // Functionality: If a ping response is not received within this duration, the connection is presumed to be lost and is closed.
        // Impact: This setting is crucial for quickly detecting unresponsive connections and freeing up resources associated with them. It ensures that the client has up-to-date information on the status of connections and can react accordingly.
        .keep_alive_timeout(Duration::from_secs(25))
        .tls_config(ClientTlsConfig::new().with_enabled_roots())?
        .connect()
        .instrument(span)
        .await?;

    Ok(channel)
}

#[cfg(any(test, feature = "test-utils"))]
mod test {
    use super::*;
    use xmtp_proto::api_client::XmtpTestClient;

    impl XmtpTestClient for GrpcClient {
        type Builder = ClientBuilder;
        fn create_local() -> Self::Builder {
            let mut client = GrpcClient::builder();
            client.set_host("http://localhost:5556".into());
            client.set_tls(false);
            client
        }

        fn create_local_d14n() -> Self::Builder {
            let mut client = GrpcClient::builder();
            client.set_host("http://localhost:5050".into());
            client.set_tls(false);
            client
        }

        fn create_local_payer() -> Self::Builder {
            let mut client = GrpcClient::builder();
            client.set_host("http://localhost:5050".into());
            client.set_tls(false);
            client
        }

        fn create_dev() -> Self::Builder {
            let mut client = GrpcClient::builder();
            client.set_host("https://grpc.dev.xmtp.network:443".into());
            client.set_tls(true);
            client
        }
    }
}
