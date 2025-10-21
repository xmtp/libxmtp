use crate::{
    mls_v1::QueryGroupMessagesResponse,
    types::{GroupId, GroupMessageMetadata, WelcomeMessage},
};

use super::*;

impl Paged for QueryGroupMessagesResponse {
    type Message = ProtoGroupMessage;
    fn info(&self) -> &Option<PagingInfo> {
        &self.paging_info
    }

    fn messages(self) -> Vec<Self::Message> {
        self.messages
    }
}

impl Paged for QueryWelcomeMessagesResponse {
    type Message = ProtoWelcomeMessage;
    fn info(&self) -> &Option<PagingInfo> {
        &self.paging_info
    }

    fn messages(self) -> Vec<Self::Message> {
        self.messages
    }
}

impl std::fmt::Debug for AggregateStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "============ Api Stats ============")?;
        writeln!(f, "UploadKeyPackage        {}", self.mls.upload_key_package)?;
        writeln!(f, "FetchKeyPackage         {}", self.mls.fetch_key_package)?;
        writeln!(
            f,
            "SendGroupMessages       {}",
            self.mls.send_group_messages
        )?;
        writeln!(
            f,
            "SendWelcomeMessages     {}",
            self.mls.send_welcome_messages
        )?;
        writeln!(
            f,
            "QueryGroupMessages      {}",
            self.mls.query_group_messages
        )?;
        writeln!(
            f,
            "QueryWelcomeMessages    {}",
            self.mls.query_welcome_messages
        )?;
        writeln!(f, "SubscribeMessages       {}", self.mls.subscribe_messages)?;
        writeln!(f, "SubscribeWelcomes       {}", self.mls.subscribe_welcomes)?;
        writeln!(f, "============ Identity ============")?;
        writeln!(
            f,
            "PublishIdentityUpdate    {}",
            self.identity.publish_identity_update
        )?;
        writeln!(
            f,
            "GetIdentityUpdatesV2     {}",
            self.identity.get_identity_updates_v2
        )?;
        writeln!(f, "GetInboxIds             {}", self.identity.get_inbox_ids)?;
        writeln!(
            f,
            "VerifySCWSignatures     {}",
            self.identity.verify_smart_contract_wallet_signature
        )?;
        writeln!(f, "============ Stream ============")?;
        writeln!(
            f,
            "SubscribeMessages        {}",
            self.mls.subscribe_messages
        )?;
        writeln!(f, "SubscribeWelcomes       {}", self.mls.subscribe_welcomes)?;
        writeln!(f, "============ Commit Log ============")?;
        writeln!(
            f,
            "PublishCommitLog         {}",
            self.mls.publish_commit_log
        )?;
        writeln!(f, "QueryCommitLog           {}", self.mls.query_commit_log)?;
        Ok(())
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<T> XmtpMlsClient for Box<T>
where
    T: XmtpMlsClient + Sync + ?Sized,
{
    type Error = <T as XmtpMlsClient>::Error;

    async fn upload_key_package(
        &self,
        request: UploadKeyPackageRequest,
    ) -> Result<(), Self::Error> {
        (**self).upload_key_package(request).await
    }

    async fn fetch_key_packages(
        &self,
        request: FetchKeyPackagesRequest,
    ) -> Result<FetchKeyPackagesResponse, Self::Error> {
        (**self).fetch_key_packages(request).await
    }

    async fn send_group_messages(
        &self,
        request: SendGroupMessagesRequest,
    ) -> Result<(), Self::Error> {
        (**self).send_group_messages(request).await
    }

    async fn send_welcome_messages(
        &self,
        request: SendWelcomeMessagesRequest,
    ) -> Result<(), Self::Error> {
        (**self).send_welcome_messages(request).await
    }

    async fn query_group_messages(
        &self,
        group_id: crate::types::GroupId,
    ) -> Result<Vec<GroupMessage>, Self::Error> {
        (**self).query_group_messages(group_id).await
    }

    async fn query_latest_group_message(
        &self,
        group_id: crate::types::GroupId,
    ) -> Result<Option<GroupMessage>, Self::Error> {
        (**self).query_latest_group_message(group_id).await
    }

    async fn query_welcome_messages(
        &self,
        installation_key: InstallationId,
    ) -> Result<Vec<WelcomeMessage>, Self::Error> {
        (**self).query_welcome_messages(installation_key).await
    }

    async fn publish_commit_log(
        &self,
        request: BatchPublishCommitLogRequest,
    ) -> Result<(), Self::Error> {
        (**self).publish_commit_log(request).await
    }

    async fn query_commit_log(
        &self,
        request: BatchQueryCommitLogRequest,
    ) -> Result<BatchQueryCommitLogResponse, Self::Error> {
        (**self).query_commit_log(request).await
    }

    async fn get_newest_group_message(
        &self,
        request: GetNewestGroupMessageRequest,
    ) -> Result<Vec<Option<GroupMessageMetadata>>, Self::Error> {
        (**self).get_newest_group_message(request).await
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<T> XmtpMlsClient for Arc<T>
where
    T: XmtpMlsClient + Sync + ?Sized + Send,
{
    type Error = <T as XmtpMlsClient>::Error;

    async fn upload_key_package(
        &self,
        request: UploadKeyPackageRequest,
    ) -> Result<(), Self::Error> {
        (**self).upload_key_package(request).await
    }

    async fn fetch_key_packages(
        &self,
        request: FetchKeyPackagesRequest,
    ) -> Result<FetchKeyPackagesResponse, Self::Error> {
        (**self).fetch_key_packages(request).await
    }

    async fn send_group_messages(
        &self,
        request: SendGroupMessagesRequest,
    ) -> Result<(), Self::Error> {
        (**self).send_group_messages(request).await
    }

    async fn send_welcome_messages(
        &self,
        request: SendWelcomeMessagesRequest,
    ) -> Result<(), Self::Error> {
        (**self).send_welcome_messages(request).await
    }

    async fn query_group_messages(
        &self,
        group_id: crate::types::GroupId,
    ) -> Result<Vec<GroupMessage>, Self::Error> {
        (**self).query_group_messages(group_id).await
    }

    async fn query_latest_group_message(
        &self,
        group_id: crate::types::GroupId,
    ) -> Result<Option<GroupMessage>, Self::Error> {
        (**self).query_latest_group_message(group_id).await
    }

    async fn query_welcome_messages(
        &self,
        installation_key: InstallationId,
    ) -> Result<Vec<WelcomeMessage>, Self::Error> {
        (**self).query_welcome_messages(installation_key).await
    }

    async fn publish_commit_log(
        &self,
        request: BatchPublishCommitLogRequest,
    ) -> Result<(), Self::Error> {
        (**self).publish_commit_log(request).await
    }

    async fn query_commit_log(
        &self,
        request: BatchQueryCommitLogRequest,
    ) -> Result<BatchQueryCommitLogResponse, Self::Error> {
        (**self).query_commit_log(request).await
    }

    async fn get_newest_group_message(
        &self,
        request: GetNewestGroupMessageRequest,
    ) -> Result<Vec<Option<GroupMessageMetadata>>, Self::Error> {
        (**self).get_newest_group_message(request).await
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<T> XmtpMlsStreams for Box<T>
where
    T: XmtpMlsStreams + Sync + ?Sized,
{
    type Error = <T as XmtpMlsStreams>::Error;

    type GroupMessageStream = <T as XmtpMlsStreams>::GroupMessageStream;

    type WelcomeMessageStream = <T as XmtpMlsStreams>::WelcomeMessageStream;

    async fn subscribe_group_messages(
        &self,
        group_ids: &[&GroupId],
    ) -> Result<Self::GroupMessageStream, Self::Error> {
        (**self).subscribe_group_messages(group_ids).await
    }

    async fn subscribe_welcome_messages(
        &self,
        installations: &[&InstallationId],
    ) -> Result<Self::WelcomeMessageStream, Self::Error> {
        (**self).subscribe_welcome_messages(installations).await
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<T> XmtpMlsStreams for Arc<T>
where
    T: XmtpMlsStreams + Sync + ?Sized + Send,
{
    type Error = <T as XmtpMlsStreams>::Error;

    type GroupMessageStream = <T as XmtpMlsStreams>::GroupMessageStream;

    type WelcomeMessageStream = <T as XmtpMlsStreams>::WelcomeMessageStream;

    async fn subscribe_group_messages(
        &self,
        group_ids: &[&GroupId],
    ) -> Result<Self::GroupMessageStream, Self::Error> {
        (**self).subscribe_group_messages(group_ids).await
    }

    async fn subscribe_welcome_messages(
        &self,
        installations: &[&InstallationId],
    ) -> Result<Self::WelcomeMessageStream, Self::Error> {
        (**self).subscribe_welcome_messages(installations).await
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<T> XmtpIdentityClient for Box<T>
where
    T: XmtpIdentityClient + Send + Sync + ?Sized,
{
    type Error = <T as XmtpIdentityClient>::Error;

    async fn publish_identity_update(
        &self,
        request: PublishIdentityUpdateRequest,
    ) -> Result<PublishIdentityUpdateResponse, Self::Error> {
        (**self).publish_identity_update(request).await
    }

    async fn get_identity_updates_v2(
        &self,
        request: GetIdentityUpdatesV2Request,
    ) -> Result<GetIdentityUpdatesV2Response, Self::Error> {
        (**self).get_identity_updates_v2(request).await
    }

    async fn get_inbox_ids(
        &self,
        request: GetInboxIdsRequest,
    ) -> Result<GetInboxIdsResponse, Self::Error> {
        (**self).get_inbox_ids(request).await
    }

    async fn verify_smart_contract_wallet_signatures(
        &self,
        request: VerifySmartContractWalletSignaturesRequest,
    ) -> Result<VerifySmartContractWalletSignaturesResponse, Self::Error> {
        (**self)
            .verify_smart_contract_wallet_signatures(request)
            .await
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<T> XmtpIdentityClient for Arc<T>
where
    T: XmtpIdentityClient + Send + Sync + ?Sized,
{
    type Error = <T as XmtpIdentityClient>::Error;

    async fn publish_identity_update(
        &self,
        request: PublishIdentityUpdateRequest,
    ) -> Result<PublishIdentityUpdateResponse, Self::Error> {
        (**self).publish_identity_update(request).await
    }

    async fn get_identity_updates_v2(
        &self,
        request: GetIdentityUpdatesV2Request,
    ) -> Result<GetIdentityUpdatesV2Response, Self::Error> {
        (**self).get_identity_updates_v2(request).await
    }

    async fn get_inbox_ids(
        &self,
        request: GetInboxIdsRequest,
    ) -> Result<GetInboxIdsResponse, Self::Error> {
        (**self).get_inbox_ids(request).await
    }

    async fn verify_smart_contract_wallet_signatures(
        &self,
        request: VerifySmartContractWalletSignaturesRequest,
    ) -> Result<VerifySmartContractWalletSignaturesResponse, Self::Error> {
        (**self)
            .verify_smart_contract_wallet_signatures(request)
            .await
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<T> CursorAwareApi for Box<T>
where
    T: CursorAwareApi + ?Sized,
{
    type CursorStore = T::CursorStore;

    fn set_cursor_store(&self, store: Self::CursorStore) {
        <T as CursorAwareApi>::set_cursor_store(self, store)
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<T> CursorAwareApi for Arc<T>
where
    T: CursorAwareApi + ?Sized,
{
    type CursorStore = T::CursorStore;

    fn set_cursor_store(&self, store: Self::CursorStore) {
        <T as CursorAwareApi>::set_cursor_store(self, store)
    }
}
