use ethers::prelude::LocalWallet;

use xmtp_proto::{
    api_client::XmtpMlsClient,
    api_client::{Error as ProtoError, ErrorKind as ProtoErrorKind},
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

pub struct XmtpXpsClient<LegacyClient> {
    /// This is the current mls client to fill in non-d14n functionality
    legacy_client: LegacyClient,
    xps: XpsOperations,
}

impl<LegacyClient> XmtpXpsClient<LegacyClient>
where
    LegacyClient: XmtpMlsClient + Send + Sync,
{
    pub async fn new<S: AsRef<str>, P: AsRef<str>>(
        endpoint: S,
        legacy_client: LegacyClient,
        owner: LocalWallet,
        network_endpoint: P,
    ) -> Result<Self, XpsClientError> {
        Ok(Self {
            legacy_client,
            xps: XpsOperations::new(endpoint, owner, network_endpoint).await?,
        })
    }
}

#[async_trait::async_trait]
impl<LegacyClient> XmtpMlsClient for XmtpXpsClient<LegacyClient>
where
    LegacyClient: XmtpMlsClient + Send + Sync,
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
        self.legacy_client.upload_key_package(request).await
    }

    async fn fetch_key_packages(
        &self,
        request: FetchKeyPackagesRequest,
    ) -> Result<FetchKeyPackagesResponse, xmtp_proto::api_client::Error> {
        self.legacy_client.fetch_key_packages(request).await
    }

    async fn send_group_messages(
        &self,
        request: SendGroupMessagesRequest,
    ) -> Result<(), xmtp_proto::api_client::Error> {
        self.legacy_client.send_group_messages(request).await
    }

    async fn send_welcome_messages(
        &self,
        request: SendWelcomeMessagesRequest,
    ) -> Result<(), xmtp_proto::api_client::Error> {
        self.legacy_client.send_welcome_messages(request).await
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
        self.legacy_client.query_group_messages(request).await
    }

    async fn query_welcome_messages(
        &self,
        request: QueryWelcomeMessagesRequest,
    ) -> Result<QueryWelcomeMessagesResponse, xmtp_proto::api_client::Error> {
        self.legacy_client.query_welcome_messages(request).await
    }

    async fn subscribe_group_messages(
        &self,
        request: SubscribeGroupMessagesRequest,
    ) -> Result<xmtp_proto::api_client::GroupMessageStream, xmtp_proto::api_client::Error> {
        self.legacy_client.subscribe_group_messages(request).await
    }

    async fn subscribe_welcome_messages(
        &self,
        request: SubscribeWelcomeMessagesRequest,
    ) -> Result<xmtp_proto::api_client::WelcomeMessageStream, xmtp_proto::api_client::Error> {
        self.legacy_client.subscribe_welcome_messages(request).await
    }
}

fn to_client_error<E>(kind: ProtoErrorKind, error: E) -> xmtp_proto::api_client::Error
where
    E: std::error::Error + Send + Sync + 'static,
{
    ProtoError::new(kind).with(error)
}
