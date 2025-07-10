use crate::streams::{EscapableTonicStream, XmtpTonicStream};
use crate::{
    apply_channel_options, create_tls_channel, GrpcBuilderError, GrpcError, GRPC_PAYLOAD_LIMIT,
};
use tonic::{metadata::MetadataValue, transport::Channel, Request};
use tower::ServiceExt;
use xmtp_proto::api_client::AggregateStats;
use xmtp_proto::api_client::{ApiBuilder, ApiStats, IdentityStats, XmtpMlsStreams};
use xmtp_proto::traits::ApiClientError;
use xmtp_proto::traits::HasStats;
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
    pub(crate) mls_client: ProtoMlsApiClient<Channel>,
    pub(crate) identity_client: ProtoIdentityApiClient<Channel>,
    pub(crate) app_version: MetadataValue<tonic::metadata::Ascii>,
    pub(crate) libxmtp_version: MetadataValue<tonic::metadata::Ascii>,
    pub(crate) stats: ApiStats,
    pub(crate) identity_stats: IdentityStats,
    pub(crate) channel: Channel,
}

impl Client {
    pub async fn create(host: impl ToString, is_secure: bool) -> Result<Self, GrpcBuilderError> {
        let mut b = Self::builder();
        b.set_tls(is_secure);
        b.set_host(host.to_string());
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

    pub fn identity_client(&self) -> &ProtoIdentityApiClient<Channel> {
        &self.identity_client
    }

    pub fn builder() -> ClientBuilder {
        ClientBuilder::default()
    }

    pub async fn is_connected(&self) -> bool {
        self.channel.clone().ready().await.is_ok()
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
    /// Rate per minute
    limit: Option<u64>,
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

    fn rate_per_minute(&mut self, limit: u32) {
        self.limit = Some(limit.into());
    }

    async fn build(self) -> Result<Self::Output, Self::Error> {
        let host = self.host.ok_or(GrpcBuilderError::MissingHostUrl)?;
        let channel = match self.tls_channel {
            true => create_tls_channel(host, self.limit.unwrap_or(1900)).await?,
            false => {
                apply_channel_options(Channel::from_shared(host)?, self.limit.unwrap_or(1900))
                    .connect()
                    .await?
            }
        };

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
                .app_version
                .unwrap_or(MetadataValue::try_from("0.0.0")?),
            libxmtp_version: self
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

#[async_trait::async_trait]
impl XmtpMlsStreams for Client {
    type Error = ApiClientError<crate::GrpcError>;
    type GroupMessageStream = XmtpTonicStream<EscapableTonicStream<GroupMessage>>;
    type WelcomeMessageStream = XmtpTonicStream<EscapableTonicStream<WelcomeMessage>>;

    async fn subscribe_group_messages(
        &self,
        req: SubscribeGroupMessagesRequest,
    ) -> Result<Self::GroupMessageStream, Self::Error> {
        self.stats.subscribe_messages.count_request();
        let client = &mut self.mls_client.clone();
        let res = client
            .subscribe_group_messages(self.build_request(req))
            .await
            .map_err(|e| ApiClientError::new(ApiEndpoint::SubscribeGroupMessages, e.into()))?;

        let stream = res.into_inner();
        Ok(XmtpTonicStream::new(
            stream.into(),
            ApiEndpoint::SubscribeGroupMessages,
        ))
    }

    async fn subscribe_welcome_messages(
        &self,
        req: SubscribeWelcomeMessagesRequest,
    ) -> Result<Self::WelcomeMessageStream, Self::Error> {
        self.stats.subscribe_welcomes.count_request();
        let client = &mut self.mls_client.clone();
        let res = client
            .subscribe_welcome_messages(self.build_request(req))
            .await
            .map_err(|e| ApiClientError::new(ApiEndpoint::SubscribeWelcomes, e.into()))?;

        let stream = res.into_inner();
        Ok(XmtpTonicStream::new(
            stream.into(),
            ApiEndpoint::SubscribeWelcomes,
        ))
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
