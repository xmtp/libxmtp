use std::pin::Pin;
use std::time::Duration;

use futures::{Stream, StreamExt};
use tonic::transport::ClientTlsConfig;
use tonic::{metadata::MetadataValue, transport::Channel, Request};
use tracing::Instrument;
use xmtp_proto::traits::ApiClientError;

use crate::{GrpcBuilderError, GrpcError};
use xmtp_proto::api_client::{ApiBuilder, ApiStats, IdentityStats, XmtpMlsStreams};
use xmtp_proto::xmtp::mls::api::v1::{GroupMessage, WelcomeMessage};
use xmtp_proto::{
    api_client::XmtpMlsClient,
    xmtp::identity::api::v1::identity_api_client::IdentityApiClient as ProtoIdentityApiClient,
    xmtp::mls::api::v1::{
        mls_api_client::MlsApiClient as ProtoMlsApiClient, FetchKeyPackagesRequest,
        FetchKeyPackagesResponse, QueryGroupMessagesRequest, QueryGroupMessagesResponse,
        QueryWelcomeMessagesRequest, QueryWelcomeMessagesResponse, SendGroupMessagesRequest,
        SendWelcomeMessagesRequest, SubscribeGroupMessagesRequest, SubscribeWelcomeMessagesRequest,
        UploadKeyPackageRequest,
    },
    ApiEndpoint,
};

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

#[derive(Debug, Clone)]
pub struct Client {
    pub(crate) mls_client: ProtoMlsApiClient<Channel>,
    pub(crate) identity_client: ProtoIdentityApiClient<Channel>,
    pub(crate) app_version: MetadataValue<tonic::metadata::Ascii>,
    pub(crate) libxmtp_version: MetadataValue<tonic::metadata::Ascii>,
    pub(crate) stats: ApiStats,
    pub(crate) identity_stats: IdentityStats,
}

impl Client {
    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn create(host: impl ToString, is_secure: bool) -> Result<Self, GrpcBuilderError> {
        let host = host.to_string();
        let app_version = MetadataValue::try_from(&String::from("0.0.0"))?;
        let libxmtp_version = MetadataValue::try_from(env!("CARGO_PKG_VERSION").to_string())?;

        let channel = match is_secure {
            true => create_tls_channel(host).await?,
            false => Channel::from_shared(host)?.connect().await?,
        };

        let mls_client = ProtoMlsApiClient::new(channel.clone());
        let identity_client = ProtoIdentityApiClient::new(channel);

        Ok(Self {
            mls_client,
            app_version,
            libxmtp_version,
            identity_client,
            stats: ApiStats::default(),
            identity_stats: IdentityStats::default(),
        })
    }

    pub fn build_request<RequestType>(&self, request: RequestType) -> Request<RequestType> {
        let mut req = Request::new(request);
        req.metadata_mut()
            .insert("x-app-version", self.app_version.clone());
        req.metadata_mut()
            .insert("x-libxmtp-version", self.libxmtp_version.clone());

        req
    }

    pub fn identity_client(&self) -> &ProtoIdentityApiClient<Channel> {
        &self.identity_client
    }

    pub fn builder() -> ClientBuilder {
        ClientBuilder::default()
    }
}

#[derive(Default)]
pub struct ClientBuilder {
    host: Option<String>,
    /// version of the app
    app_version: Option<MetadataValue<tonic::metadata::Ascii>>,
    /// Version of the libxmtp core library
    libxmtp_version: Option<MetadataValue<tonic::metadata::Ascii>>,
    /// Whether or not the channel should use TLS
    tls_channel: bool,
}

impl ApiBuilder for ClientBuilder {
    type Output = Client;
    type Error = crate::GrpcBuilderError;

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

        let mls_client = ProtoMlsApiClient::new(channel.clone());
        let identity_client = ProtoIdentityApiClient::new(channel);

        Ok(Client {
            mls_client,
            identity_client,
            app_version: self
                .app_version
                .unwrap_or(MetadataValue::try_from("0.0.0")?),
            libxmtp_version: self
                .libxmtp_version
                .unwrap_or(MetadataValue::try_from(env!("CARGO_PKG_VERSION"))?),

            stats: ApiStats::default(),
            identity_stats: IdentityStats::default(),
        })
    }
}

#[async_trait::async_trait]
impl XmtpMlsClient for Client {
    type Error = ApiClientError<GrpcError>;

    #[tracing::instrument(level = "trace", skip_all)]
    async fn upload_key_package(&self, req: UploadKeyPackageRequest) -> Result<(), Self::Error> {
        self.stats.upload_key_package.count_request();
        let client = &mut self.mls_client.clone();

        client
            .upload_key_package(self.build_request(req))
            .await
            .map_err(|e| ApiClientError::new(ApiEndpoint::UploadKeyPackage, e.into()))?;
        Ok(())
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn fetch_key_packages(
        &self,
        req: FetchKeyPackagesRequest,
    ) -> Result<FetchKeyPackagesResponse, Self::Error> {
        self.stats.fetch_key_package.count_request();
        let client = &mut self.mls_client.clone();
        let res = client.fetch_key_packages(self.build_request(req)).await;

        res.map(|r| r.into_inner())
            .map_err(|e| ApiClientError::new(ApiEndpoint::FetchKeyPackages, e.into()))
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn send_group_messages(&self, req: SendGroupMessagesRequest) -> Result<(), Self::Error> {
        self.stats.send_group_messages.count_request();
        let client = &mut self.mls_client.clone();
        client
            .send_group_messages(self.build_request(req))
            .await
            .map_err(|e| ApiClientError::new(ApiEndpoint::SendGroupMessages, e.into()))?;
        Ok(())
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn send_welcome_messages(
        &self,
        req: SendWelcomeMessagesRequest,
    ) -> Result<(), Self::Error> {
        self.stats.send_welcome_messages.count_request();
        let client = &mut self.mls_client.clone();
        client
            .send_welcome_messages(self.build_request(req))
            .await
            .map_err(|e| ApiClientError::new(ApiEndpoint::SendWelcomeMessages, e.into()))?;
        Ok(())
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn query_group_messages(
        &self,
        req: QueryGroupMessagesRequest,
    ) -> Result<QueryGroupMessagesResponse, Self::Error> {
        self.stats.query_group_messages.count_request();
        let client = &mut self.mls_client.clone();
        client
            .query_group_messages(self.build_request(req))
            .await
            .map(|r| r.into_inner())
            .map_err(|e| ApiClientError::new(ApiEndpoint::QueryGroupMessages, e.into()))
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn query_welcome_messages(
        &self,
        req: QueryWelcomeMessagesRequest,
    ) -> Result<QueryWelcomeMessagesResponse, Self::Error> {
        self.stats.query_welcome_messages.count_request();
        let client = &mut self.mls_client.clone();
        client
            .query_welcome_messages(self.build_request(req))
            .await
            .map(|r| r.into_inner())
            .map_err(|e| ApiClientError::new(ApiEndpoint::QueryWelcomeMessages, e.into()))
    }

    fn stats(&self) -> ApiStats {
        self.stats.clone()
    }
}

pub struct GroupMessageStream {
    inner: tonic::codec::Streaming<GroupMessage>,
}

impl From<tonic::codec::Streaming<GroupMessage>> for GroupMessageStream {
    fn from(inner: tonic::codec::Streaming<GroupMessage>) -> Self {
        GroupMessageStream { inner }
    }
}

impl Stream for GroupMessageStream {
    type Item = Result<GroupMessage, ApiClientError<crate::GrpcError>>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.inner.poll_next_unpin(cx).map(|data| {
            data.map(|v| {
                v.map_err(|e| ApiClientError::new(ApiEndpoint::SubscribeGroupMessages, e.into()))
            })
        })
    }
}

pub struct WelcomeMessageStream {
    inner: tonic::codec::Streaming<WelcomeMessage>,
}

impl From<tonic::codec::Streaming<WelcomeMessage>> for WelcomeMessageStream {
    fn from(inner: tonic::codec::Streaming<WelcomeMessage>) -> Self {
        WelcomeMessageStream { inner }
    }
}

impl Stream for WelcomeMessageStream {
    type Item = Result<WelcomeMessage, ApiClientError<crate::GrpcError>>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.inner.poll_next_unpin(cx).map(|data| {
            data.map(|v| {
                v.map_err(|e| ApiClientError::new(ApiEndpoint::SubscribeWelcomes, e.into()))
            })
        })
    }
}

#[async_trait::async_trait]
impl XmtpMlsStreams for Client {
    type Error = ApiClientError<crate::GrpcError>;
    type GroupMessageStream<'a> = GroupMessageStream;
    type WelcomeMessageStream<'a> = WelcomeMessageStream;

    async fn subscribe_group_messages(
        &self,
        req: SubscribeGroupMessagesRequest,
    ) -> Result<Self::GroupMessageStream<'_>, Self::Error> {
        let client = &mut self.mls_client.clone();
        let res = client
            .subscribe_group_messages(self.build_request(req))
            .await
            .map_err(|e| ApiClientError::new(ApiEndpoint::SubscribeGroupMessages, e.into()))?;

        let stream = res.into_inner();
        Ok(stream.into())
    }

    async fn subscribe_welcome_messages(
        &self,
        req: SubscribeWelcomeMessagesRequest,
    ) -> Result<Self::WelcomeMessageStream<'_>, Self::Error> {
        let client = &mut self.mls_client.clone();
        let res = client
            .subscribe_welcome_messages(self.build_request(req))
            .await
            .map_err(|e| ApiClientError::new(ApiEndpoint::SubscribeWelcomes, e.into()))?;

        let stream = res.into_inner();

        Ok(stream.into())
    }
}

#[cfg(any(test, feature = "test-utils"))]
mod test {
    use super::*;
    use xmtp_proto::api_client::XmtpTestClient;

    impl XmtpTestClient for Client {
        type Builder = ClientBuilder;
        fn create_local() -> Self::Builder {
            let mut client = Client::builder();
            client.set_host("http://localhost:5556".into());
            client.set_tls(false);
            client
        }

        fn create_local_d14n() -> Self::Builder {
            let mut client = Client::builder();
            client.set_host("http://localhost:5050".into());
            client.set_tls(false);
            client
        }

        fn create_local_payer() -> Self::Builder {
            let mut client = Client::builder();
            client.set_host("http://localhost:5050".into());
            client.set_tls(false);
            client
        }

        fn create_dev() -> Self::Builder {
            let mut client = Client::builder();
            client.set_host("https://grpc.dev.xmtp.network:443".into());
            client.set_tls(true);
            client
        }
    }
}
