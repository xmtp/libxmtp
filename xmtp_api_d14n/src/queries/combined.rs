use xmtp_proto::api_client::XmtpMlsClient;
use xmtp_proto::identity_v1;
use xmtp_proto::mls_v1;
use xmtp_proto::prelude::XmtpIdentityClient;
use xmtp_proto::types::InstallationId;
use xmtp_proto::types::WelcomeMessage;
use xmtp_proto::types::{Cursor, GroupId, GroupMessage};

#[derive(Clone)]
pub struct CombinedD14nClient<C, D> {
    pub(crate) v3_client: C,
    pub(crate) xmtpd_client: D,
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<C, D> XmtpMlsClient for CombinedD14nClient<C, D>
where
    C: Send + Sync + XmtpMlsClient,
    D: Send + Sync + XmtpMlsClient<Error = C::Error>,
{
    type Error = <C as XmtpMlsClient>::Error;

    async fn upload_key_package(
        &self,
        request: mls_v1::UploadKeyPackageRequest,
    ) -> Result<(), Self::Error> {
        self.v3_client.upload_key_package(request).await
    }

    async fn fetch_key_packages(
        &self,
        request: mls_v1::FetchKeyPackagesRequest,
    ) -> Result<mls_v1::FetchKeyPackagesResponse, Self::Error> {
        self.xmtpd_client.fetch_key_packages(request).await
    }

    async fn send_group_messages(
        &self,
        request: mls_v1::SendGroupMessagesRequest,
    ) -> Result<(), Self::Error> {
        self.v3_client.send_group_messages(request).await
    }

    async fn send_welcome_messages(
        &self,
        request: mls_v1::SendWelcomeMessagesRequest,
    ) -> Result<(), Self::Error> {
        self.v3_client.send_welcome_messages(request).await
    }
    async fn query_group_messages(
        &self,
        group_id: GroupId,
        cursor: Vec<Cursor>,
    ) -> Result<Vec<GroupMessage>, Self::Error> {
        self.xmtpd_client
            .query_group_messages(group_id, cursor)
            .await
    }

    async fn query_latest_group_message(
        &self,
        group_id: GroupId,
    ) -> Result<Option<GroupMessage>, Self::Error> {
        self.xmtpd_client.query_latest_group_message(group_id).await
    }

    async fn query_welcome_messages(
        &self,
        installation_key: InstallationId,
        cursor: Vec<Cursor>,
    ) -> Result<Vec<WelcomeMessage>, Self::Error> {
        self.xmtpd_client
            .query_welcome_messages(installation_key, cursor)
            .await
    }

    async fn publish_commit_log(
        &self,
        request: mls_v1::BatchPublishCommitLogRequest,
    ) -> Result<(), Self::Error> {
        self.xmtpd_client.publish_commit_log(request).await
    }

    async fn query_commit_log(
        &self,
        request: mls_v1::BatchQueryCommitLogRequest,
    ) -> Result<mls_v1::BatchQueryCommitLogResponse, Self::Error> {
        self.v3_client.query_commit_log(request).await
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<C, D> XmtpIdentityClient for CombinedD14nClient<C, D>
where
    C: Send + Sync + XmtpIdentityClient,
    D: Send + Sync + XmtpIdentityClient<Error = C::Error>,
{
    type Error = <C as XmtpIdentityClient>::Error;

    async fn publish_identity_update(
        &self,
        request: identity_v1::PublishIdentityUpdateRequest,
    ) -> Result<identity_v1::PublishIdentityUpdateResponse, Self::Error> {
        self.v3_client.publish_identity_update(request).await
    }

    async fn get_identity_updates_v2(
        &self,
        request: identity_v1::GetIdentityUpdatesRequest,
    ) -> Result<identity_v1::GetIdentityUpdatesResponse, Self::Error> {
        self.xmtpd_client.get_identity_updates_v2(request).await
    }

    async fn get_inbox_ids(
        &self,
        request: identity_v1::GetInboxIdsRequest,
    ) -> Result<identity_v1::GetInboxIdsResponse, Self::Error> {
        self.xmtpd_client.get_inbox_ids(request).await
    }

    async fn verify_smart_contract_wallet_signatures(
        &self,
        request: identity_v1::VerifySmartContractWalletSignaturesRequest,
    ) -> Result<identity_v1::VerifySmartContractWalletSignaturesResponse, Self::Error> {
        self.xmtpd_client
            .verify_smart_contract_wallet_signatures(request)
            .await
    }
}
