use ethers::prelude::LocalWallet;

use xmtp_api_grpc::grpc_api_helper::{
    Client as GrpcHelperClient, GrpcMutableSubscription, Subscription,
};
use xmtp_proto::{
    api_client::{Error as ProtoError, ErrorKind as ProtoErrorKind},
    api_client::{XmtpApiClient, XmtpMlsClient},
    xmtp::message_api::v1::{
        BatchQueryRequest, BatchQueryResponse, PublishRequest, PublishResponse, QueryRequest,
        QueryResponse, SubscribeRequest,
    },
    xmtp::mls::api::v1::{
        FetchKeyPackagesRequest, FetchKeyPackagesResponse, GetIdentityUpdatesRequest,
        GetIdentityUpdatesResponse, QueryGroupMessagesRequest, QueryGroupMessagesResponse,
        QueryWelcomeMessagesRequest, QueryWelcomeMessagesResponse, RegisterInstallationRequest,
        RegisterInstallationResponse, SendGroupMessagesRequest, SendWelcomeMessagesRequest,
        SubscribeGroupMessagesRequest, SubscribeWelcomeMessagesRequest, UploadKeyPackageRequest,
    },
};

pub mod xps_operations;
use xps_operations::{XpsClientError, XpsOperations};

pub struct XmtpXpsClient<WakuClient> {
    /// This is the current mls client to fill in non-d14n functionality
    waku_client: WakuClient,
    xps: XpsOperations,
}

impl<WakuClient> XmtpXpsClient<WakuClient>
where
    WakuClient: XmtpMlsClient + XmtpApiClient + Send + Sync,
{
    pub async fn new<S: AsRef<str>, P: AsRef<str>>(
        waku_client: WakuClient,
        owner: LocalWallet,
        endpoint: S,
        network_endpoint: P,
    ) -> Result<Self, XpsClientError> {
        Ok(Self {
            waku_client,
            xps: XpsOperations::new(endpoint, owner, network_endpoint).await?,
        })
    }
}

#[async_trait::async_trait]
impl<WakuClient> XmtpMlsClient for XmtpXpsClient<WakuClient>
where
    WakuClient: XmtpMlsClient + Send + Sync,
{
    async fn register_installation(
        &self,
        request: RegisterInstallationRequest,
    ) -> Result<RegisterInstallationResponse, xmtp_proto::api_client::Error> {
        self.xps
            .register_installation(request)
            .await
            .map_err(|e| to_client_error(ProtoErrorKind::PublishError, e))
    }

    async fn upload_key_package(
        &self,
        request: UploadKeyPackageRequest,
    ) -> Result<(), xmtp_proto::api_client::Error> {
        self.waku_client.upload_key_package(request).await
    }

    async fn fetch_key_packages(
        &self,
        request: FetchKeyPackagesRequest,
    ) -> Result<FetchKeyPackagesResponse, xmtp_proto::api_client::Error> {
        self.waku_client.fetch_key_packages(request).await
    }

    async fn send_group_messages(
        &self,
        request: SendGroupMessagesRequest,
    ) -> Result<(), xmtp_proto::api_client::Error> {
        self.waku_client.send_group_messages(request).await
    }

    async fn send_welcome_messages(
        &self,
        request: SendWelcomeMessagesRequest,
    ) -> Result<(), xmtp_proto::api_client::Error> {
        self.waku_client.send_welcome_messages(request).await
    }

    async fn get_identity_updates(
        &self,
        request: GetIdentityUpdatesRequest,
    ) -> Result<GetIdentityUpdatesResponse, xmtp_proto::api_client::Error> {
        // TODO: Allow filtering by start_time_ns
        // fetch key packages JSON-RPC (needs a rename)
        self.xps
            .get_identity_updates(request)
            .await
            .map_err(|e| to_client_error(ProtoErrorKind::QueryError, e))
    }

    async fn query_group_messages(
        &self,
        request: QueryGroupMessagesRequest,
    ) -> Result<QueryGroupMessagesResponse, xmtp_proto::api_client::Error> {
        self.waku_client.query_group_messages(request).await
    }

    async fn query_welcome_messages(
        &self,
        request: QueryWelcomeMessagesRequest,
    ) -> Result<QueryWelcomeMessagesResponse, xmtp_proto::api_client::Error> {
        self.waku_client.query_welcome_messages(request).await
    }

    async fn subscribe_group_messages(
        &self,
        request: SubscribeGroupMessagesRequest,
    ) -> Result<xmtp_proto::api_client::GroupMessageStream, xmtp_proto::api_client::Error> {
        self.waku_client.subscribe_group_messages(request).await
    }

    async fn subscribe_welcome_messages(
        &self,
        request: SubscribeWelcomeMessagesRequest,
    ) -> Result<xmtp_proto::api_client::WelcomeMessageStream, xmtp_proto::api_client::Error> {
        self.waku_client.subscribe_welcome_messages(request).await
    }
}

#[async_trait::async_trait]
impl XmtpApiClient for XmtpXpsClient<GrpcHelperClient> {
    type Subscription = Subscription;

    type MutableSubscription = GrpcMutableSubscription;

    fn set_app_version(&mut self, version: String) {
        self.waku_client.set_app_version(version);
    }

    async fn publish(
        &self,
        token: String,
        request: PublishRequest,
    ) -> Result<PublishResponse, ProtoError> {
        self.waku_client.publish(token, request).await
    }

    async fn subscribe(&self, request: SubscribeRequest) -> Result<Self::Subscription, ProtoError> {
        self.waku_client.subscribe(request).await
    }

    async fn subscribe2(
        &self,
        request: SubscribeRequest,
    ) -> Result<Self::MutableSubscription, ProtoError> {
        self.waku_client.subscribe2(request).await
    }

    async fn query(&self, request: QueryRequest) -> Result<QueryResponse, ProtoError> {
        self.waku_client.query(request).await
    }

    async fn batch_query(
        &self,
        request: BatchQueryRequest,
    ) -> Result<BatchQueryResponse, ProtoError> {
        self.waku_client.batch_query(request).await
    }
}

fn to_client_error<E>(kind: ProtoErrorKind, error: E) -> xmtp_proto::api_client::Error
where
    E: std::error::Error + Send + Sync + 'static,
{
    ProtoError::new(kind).with(error)
}
