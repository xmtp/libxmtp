use xmtp_api_grpc::error::GrpcError;
use xmtp_configuration::CUTOVER_REFRESH_TIME;
use xmtp_proto::api::ApiClientError;
use xmtp_proto::api::Client;
use xmtp_proto::api_client::XmtpMlsClient;
use xmtp_proto::identity_v1;
use xmtp_proto::mls_v1;
use xmtp_proto::prelude::XmtpIdentityClient;
use xmtp_proto::prelude::*;
use xmtp_proto::types::InstallationId;
use xmtp_proto::types::WelcomeMessage;
use xmtp_proto::types::{GroupId, GroupMessage};

use crate::d14n::FetchD14nCutover;
use crate::protocol::CursorStore;
use crate::protocol::FullXmtpApiArc;

mod streams;

type XmtpApiClient = FullXmtpApiArc<ApiClientError<GrpcError>>;

#[derive(Clone)]
pub struct CombinedD14nClient<C, Store> {
    pub(crate) v3_client: XmtpApiClient,
    pub(crate) xmtpd_client: XmtpApiClient,
    store: Store,
    client: C,
}

impl<C, S: CursorStore> CombinedD14nClient<C, S>
where
    C: Client<Error = GrpcError>,
{
    pub async fn choose_client(&self) -> Result<&XmtpApiClient, ApiClientError<GrpcError>> {
        if self.store.has_migrated()? {
            return Ok(&self.xmtpd_client);
        }

        let now = xmtp_common::time::now_ns();
        let cutover_ns = self.store.get_cutover_ns()?;

        let time_since_refresh = now.saturating_sub(cutover_ns);
        let cutover_ns = if time_since_refresh >= CUTOVER_REFRESH_TIME {
            let cutover_ns = FetchD14nCutover.query(&self.client).await?.timestamp_ns as i64;
            self.store.set_cutover_ns(cutover_ns)?;
            cutover_ns
        } else {
            cutover_ns
        };

        if now >= cutover_ns {
            self.store.set_has_migrated(true)?;
            Ok(&self.xmtpd_client)
        } else {
            Ok(&self.v3_client)
        }
    }
}

#[xmtp_common::async_trait]
impl<C, S> XmtpMlsClient for CombinedD14nClient<C, S>
where
    C: Client<Error = GrpcError>,
    S: CursorStore,
{
    type Error = ApiClientError<GrpcError>;

    async fn upload_key_package(
        &self,
        request: mls_v1::UploadKeyPackageRequest,
    ) -> Result<(), Self::Error> {
        self.choose_client()
            .await?
            .upload_key_package(request)
            .await
    }

    async fn fetch_key_packages(
        &self,
        request: mls_v1::FetchKeyPackagesRequest,
    ) -> Result<mls_v1::FetchKeyPackagesResponse, Self::Error> {
        self.choose_client()
            .await?
            .fetch_key_packages(request)
            .await
    }

    async fn send_group_messages(
        &self,
        request: mls_v1::SendGroupMessagesRequest,
    ) -> Result<(), Self::Error> {
        self.choose_client()
            .await?
            .send_group_messages(request)
            .await
    }

    async fn send_welcome_messages(
        &self,
        request: mls_v1::SendWelcomeMessagesRequest,
    ) -> Result<(), Self::Error> {
        self.choose_client()
            .await?
            .send_welcome_messages(request)
            .await
    }
    async fn query_group_messages(
        &self,
        group_id: GroupId,
    ) -> Result<Vec<GroupMessage>, Self::Error> {
        self.choose_client()
            .await?
            .query_group_messages(group_id)
            .await
    }

    async fn query_latest_group_message(
        &self,
        group_id: GroupId,
    ) -> Result<Option<GroupMessage>, Self::Error> {
        self.choose_client()
            .await?
            .query_latest_group_message(group_id)
            .await
    }

    async fn query_welcome_messages(
        &self,
        installation_key: InstallationId,
    ) -> Result<Vec<WelcomeMessage>, Self::Error> {
        self.choose_client()
            .await?
            .query_welcome_messages(installation_key)
            .await
    }

    async fn publish_commit_log(
        &self,
        request: mls_v1::BatchPublishCommitLogRequest,
    ) -> Result<(), Self::Error> {
        self.choose_client()
            .await?
            .publish_commit_log(request)
            .await
    }

    async fn query_commit_log(
        &self,
        request: mls_v1::BatchQueryCommitLogRequest,
    ) -> Result<mls_v1::BatchQueryCommitLogResponse, Self::Error> {
        self.choose_client().await?.query_commit_log(request).await
    }

    async fn get_newest_group_message(
        &self,
        request: mls_v1::GetNewestGroupMessageRequest,
    ) -> Result<Vec<Option<xmtp_proto::types::GroupMessageMetadata>>, Self::Error> {
        self.choose_client()
            .await?
            .get_newest_group_message(request)
            .await
    }
}

#[xmtp_common::async_trait]
impl<C, S> XmtpIdentityClient for CombinedD14nClient<C, S>
where
    S: CursorStore,
    C: Client<Error = GrpcError>,
{
    type Error = ApiClientError<GrpcError>;

    async fn publish_identity_update(
        &self,
        request: identity_v1::PublishIdentityUpdateRequest,
    ) -> Result<identity_v1::PublishIdentityUpdateResponse, Self::Error> {
        self.choose_client()
            .await?
            .publish_identity_update(request)
            .await
    }

    async fn get_identity_updates_v2(
        &self,
        request: identity_v1::GetIdentityUpdatesRequest,
    ) -> Result<identity_v1::GetIdentityUpdatesResponse, Self::Error> {
        self.choose_client()
            .await?
            .get_identity_updates_v2(request)
            .await
    }

    async fn get_inbox_ids(
        &self,
        request: identity_v1::GetInboxIdsRequest,
    ) -> Result<identity_v1::GetInboxIdsResponse, Self::Error> {
        self.choose_client().await?.get_inbox_ids(request).await
    }

    async fn verify_smart_contract_wallet_signatures(
        &self,
        request: identity_v1::VerifySmartContractWalletSignaturesRequest,
    ) -> Result<identity_v1::VerifySmartContractWalletSignaturesResponse, Self::Error> {
        self.choose_client()
            .await?
            .verify_smart_contract_wallet_signatures(request)
            .await
    }
}
