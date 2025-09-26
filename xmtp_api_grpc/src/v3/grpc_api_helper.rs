use crate::error::{GrpcBuilderError, GrpcError};
use crate::streams::{self, XmtpTonicStream};
use tonic::{metadata::MetadataValue, Request};
use tower::ServiceExt;
use xmtp_configuration::GRPC_PAYLOAD_LIMIT;
use xmtp_proto::api_client::AggregateStats;
use xmtp_proto::api_client::{ApiBuilder, ApiStats, IdentityStats, XmtpMlsStreams};
use xmtp_proto::mls_v1::{
    BatchPublishCommitLogRequest, BatchQueryCommitLogRequest, BatchQueryCommitLogResponse,
};
use xmtp_proto::traits::ApiClientError;
use xmtp_proto::traits::HasStats;
use xmtp_proto::types::AppVersion;
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

#[derive(Debug, Clone)]
pub struct Client {
    pub(crate) mls_client: ProtoMlsApiClient<crate::GrpcService>,
    pub(crate) identity_client: ProtoIdentityApiClient<crate::GrpcService>,
    pub(crate) app_version: MetadataValue<tonic::metadata::Ascii>,
    pub(crate) libxmtp_version: MetadataValue<tonic::metadata::Ascii>,
    pub(crate) stats: ApiStats,
    pub(crate) identity_stats: IdentityStats,
    pub(crate) channel: crate::GrpcService,
}

impl Client {
    /// Create an API Client
    /// Automatically chooses gRPC service based on target.
    ///
    /// _NOTE:_ 'is_secure' is a no-op in web-assembly (browser handles TLS)
    pub async fn create(
        host: impl ToString,
        is_secure: bool,
        app_version: Option<impl ToString>,
    ) -> Result<Self, GrpcBuilderError> {
        let mut b = Self::builder();
        b.set_tls(is_secure);
        b.set_host(host.to_string());
        b.set_app_version(app_version.map_or(Default::default(), |v| v.to_string().into()))?;
        b.build().await
    }

    pub fn build_request<RequestType>(&self, request: RequestType) -> Request<RequestType> {
        let mut req = Request::new(request);
        req.metadata_mut()
            .insert("x-app-version", self.app_version.clone());
        req.metadata_mut()
            .insert("x-libxmtp-version", self.libxmtp_version.clone());
        req
    }

    pub fn identity_client(&self) -> &ProtoIdentityApiClient<crate::GrpcService> {
        &self.identity_client
    }

    pub fn builder() -> ClientBuilder {
        ClientBuilder::default()
    }

    pub async fn is_connected(&self) -> bool {
        self.channel.clone().ready().await.is_ok()
    }

    fn client(&self) -> crate::GrpcClient {
        crate::GrpcClient::new(
            self.channel.clone(),
            self.app_version.clone(),
            self.libxmtp_version.clone(),
        )
    }
}

#[derive(Default)]
pub struct ClientBuilder {
    inner: crate::ClientBuilder,
}

impl ApiBuilder for ClientBuilder {
    type Output = Client;
    type Error = crate::error::GrpcBuilderError;

    fn set_libxmtp_version(&mut self, version: String) -> Result<(), Self::Error> {
        self.inner.set_libxmtp_version(version)
    }

    fn set_app_version(&mut self, version: AppVersion) -> Result<(), Self::Error> {
        self.inner.set_app_version(version)
    }

    fn set_host(&mut self, host: String) {
        self.inner.set_host(host)
    }

    fn set_tls(&mut self, tls: bool) {
        self.inner.set_tls(tls)
    }

    fn rate_per_minute(&mut self, limit: u32) {
        self.inner.rate_per_minute(limit)
    }

    fn port(&self) -> Result<Option<String>, Self::Error> {
        self.inner.port()
    }

    fn host(&self) -> Option<&str> {
        self.inner.host()
    }

    async fn build(self) -> Result<Self::Output, Self::Error> {
        let host = self.inner.host().ok_or(GrpcBuilderError::MissingHostUrl)?;
        tracing::info!("building api client for {}", host);
        let channel =
            crate::GrpcService::new(host.to_string(), self.inner.limit, self.inner.tls_channel)
                .await?;
        let mls_client = ProtoMlsApiClient::new(channel.clone())
            .max_decoding_message_size(GRPC_PAYLOAD_LIMIT)
            .max_encoding_message_size(GRPC_PAYLOAD_LIMIT);
        let identity_client = ProtoIdentityApiClient::new(channel.clone())
            .max_decoding_message_size(GRPC_PAYLOAD_LIMIT)
            .max_encoding_message_size(GRPC_PAYLOAD_LIMIT);

        Ok(Client {
            mls_client,
            identity_client,
            app_version: self
                .inner
                .app_version
                .unwrap_or(MetadataValue::try_from("0.0.0")?),
            libxmtp_version: self
                .inner
                .libxmtp_version
                .unwrap_or(MetadataValue::try_from(env!("CARGO_PKG_VERSION"))?),
            stats: ApiStats::default(),
            identity_stats: IdentityStats::default(),
            channel,
        })
    }
}

impl HasStats for Client {
    fn aggregate_stats(&self) -> AggregateStats {
        AggregateStats {
            mls: self.stats.clone(),
            identity: self.identity_stats.clone(),
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
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

    async fn publish_commit_log(
        &self,
        req: BatchPublishCommitLogRequest,
    ) -> Result<(), Self::Error> {
        self.stats.publish_commit_log.count_request();
        let client = &mut self.mls_client.clone();
        client
            .batch_publish_commit_log(self.build_request(req))
            .await
            .map_err(|e| ApiClientError::new(ApiEndpoint::PublishCommitLog, e.into()))?;
        Ok(())
    }

    async fn query_commit_log(
        &self,
        req: BatchQueryCommitLogRequest,
    ) -> Result<BatchQueryCommitLogResponse, Self::Error> {
        self.stats.query_commit_log.count_request();
        let client = &mut self.mls_client.clone();
        client
            .batch_query_commit_log(self.build_request(req))
            .await
            .map(|r| r.into_inner())
            .map_err(|e| ApiClientError::new(ApiEndpoint::QueryCommitLog, e.into()))
    }

    fn stats(&self) -> ApiStats {
        self.stats.clone()
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl XmtpMlsStreams for Client {
    type Error = ApiClientError<crate::error::GrpcError>;
    type GroupMessageStream = streams::XmtpStream<GroupMessage>;
    type WelcomeMessageStream = streams::XmtpStream<WelcomeMessage>;

    async fn subscribe_group_messages(
        &self,
        req: SubscribeGroupMessagesRequest,
    ) -> Result<Self::GroupMessageStream, Self::Error> {
        self.stats.subscribe_messages.count_request();
        XmtpTonicStream::from_body(req, self.client(), ApiEndpoint::SubscribeGroupMessages).await
    }

    async fn subscribe_welcome_messages(
        &self,
        req: SubscribeWelcomeMessagesRequest,
    ) -> Result<Self::WelcomeMessageStream, Self::Error> {
        self.stats.subscribe_welcomes.count_request();
        XmtpTonicStream::from_body(req, self.client(), ApiEndpoint::SubscribeWelcomes).await
    }
}

#[cfg(any(test, feature = "test-utils"))]
#[allow(clippy::unwrap_used)]
mod test {
    use super::*;
    use xmtp_configuration::GrpcUrls;
    use xmtp_configuration::LOCALHOST;
    use xmtp_proto::{api_client::XmtpTestClient, TestApiBuilder, ToxicProxies};

    #[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
    #[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
    impl XmtpTestClient for Client {
        type Builder = ClientBuilder;

        fn create_local() -> Self::Builder {
            let mut client = Client::builder();
            let url = url::Url::parse(GrpcUrls::NODE).unwrap();
            match url.scheme() {
                "https" => client.set_tls(true),
                _ => client.set_tls(false),
            }
            client.set_host(GrpcUrls::NODE.into());
            client
        }

        fn create_d14n() -> Self::Builder {
            let mut client = Client::builder();
            let url = url::Url::parse(GrpcUrls::XMTPD).unwrap();
            match url.scheme() {
                "https" => client.set_tls(true),
                _ => client.set_tls(false),
            }
            client.set_host(GrpcUrls::XMTPD.into());
            client
        }

        fn create_payer() -> Self::Builder {
            let mut payer = Client::builder();
            let url = url::Url::parse(GrpcUrls::PAYER).unwrap();
            match url.scheme() {
                "https" => payer.set_tls(true),
                _ => payer.set_tls(false),
            }
            payer.set_host(GrpcUrls::PAYER.into());
            payer.set_tls(false);
            payer
        }

        fn create_dev() -> Self::Builder {
            let mut client = Client::builder();
            client.set_host(GrpcUrls::NODE_DEV.into());
            client.set_tls(true);
            client
        }
    }

    impl TestApiBuilder for ClientBuilder {
        async fn with_toxiproxy(&mut self) -> ToxicProxies {
            let proxy = xmtp_proto::init_toxi(&[self.host().unwrap()]).await;
            self.set_host(format!("{LOCALHOST}:{}", proxy.ports()[0]));
            tracing::info!("new host with toxiproxy={:?}", self.host());
            proxy
        }
    }
}
