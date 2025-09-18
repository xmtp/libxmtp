use std::pin::Pin;

use xmtp_proto::api::HasStats;
use xmtp_proto::api::IsConnectedCheck;
use xmtp_proto::api_client::ApiStats;
use xmtp_proto::api_client::CursorAwareApi;
use xmtp_proto::api_client::IdentityStats;
use xmtp_proto::api_client::XmtpMlsClient;
use xmtp_proto::identity_v1;
use xmtp_proto::mls_v1;
use xmtp_proto::prelude::XmtpIdentityClient;
use xmtp_proto::prelude::XmtpMlsStreams;
use xmtp_proto::types::InstallationId;
use xmtp_proto::types::WelcomeMessage;
use xmtp_proto::types::{GroupId, GroupMessage};

use crate::protocol::XmtpQuery;

/// Wraps an ApiClient to allow turning
/// a concretely-typed client into type-erased a [`BoxableXmtpApi`]
/// allowing for the transformation into a type-erased Api Client
#[derive(Clone)]
pub struct BoxedStreamsClient<C> {
    inner: C,
}

impl<C> BoxedStreamsClient<C> {
    pub fn new(inner: C) -> Self {
        Self { inner }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<C> XmtpMlsClient for BoxedStreamsClient<C>
where
    C: Send + Sync + XmtpMlsClient,
{
    type Error = <C as XmtpMlsClient>::Error;

    async fn upload_key_package(
        &self,
        request: mls_v1::UploadKeyPackageRequest,
    ) -> Result<(), Self::Error> {
        self.inner.upload_key_package(request).await
    }

    async fn fetch_key_packages(
        &self,
        request: mls_v1::FetchKeyPackagesRequest,
    ) -> Result<mls_v1::FetchKeyPackagesResponse, Self::Error> {
        self.inner.fetch_key_packages(request).await
    }

    async fn send_group_messages(
        &self,
        request: mls_v1::SendGroupMessagesRequest,
    ) -> Result<(), Self::Error> {
        self.inner.send_group_messages(request).await
    }

    async fn send_welcome_messages(
        &self,
        request: mls_v1::SendWelcomeMessagesRequest,
    ) -> Result<(), Self::Error> {
        self.inner.send_welcome_messages(request).await
    }
    async fn query_group_messages(
        &self,
        group_id: GroupId,
    ) -> Result<Vec<GroupMessage>, Self::Error> {
        self.inner.query_group_messages(group_id).await
    }

    async fn query_latest_group_message(
        &self,
        group_id: GroupId,
    ) -> Result<Option<GroupMessage>, Self::Error> {
        self.inner.query_latest_group_message(group_id).await
    }

    async fn query_welcome_messages(
        &self,
        installation_key: InstallationId,
    ) -> Result<Vec<WelcomeMessage>, Self::Error> {
        self.inner.query_welcome_messages(installation_key).await
    }

    async fn publish_commit_log(
        &self,
        request: mls_v1::BatchPublishCommitLogRequest,
    ) -> Result<(), Self::Error> {
        self.inner.publish_commit_log(request).await
    }

    async fn query_commit_log(
        &self,
        request: mls_v1::BatchQueryCommitLogRequest,
    ) -> Result<mls_v1::BatchQueryCommitLogResponse, Self::Error> {
        self.inner.query_commit_log(request).await
    }

    async fn get_newest_group_message(
        &self,
        request: mls_v1::GetNewestGroupMessageRequest,
    ) -> Result<Vec<Option<xmtp_proto::types::GroupMessageMetadata>>, Self::Error> {
        self.inner.get_newest_group_message(request).await
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<C> XmtpIdentityClient for BoxedStreamsClient<C>
where
    C: Send + Sync + XmtpIdentityClient,
{
    type Error = <C as XmtpIdentityClient>::Error;

    async fn publish_identity_update(
        &self,
        request: identity_v1::PublishIdentityUpdateRequest,
    ) -> Result<identity_v1::PublishIdentityUpdateResponse, Self::Error> {
        self.inner.publish_identity_update(request).await
    }

    async fn get_identity_updates_v2(
        &self,
        request: identity_v1::GetIdentityUpdatesRequest,
    ) -> Result<identity_v1::GetIdentityUpdatesResponse, Self::Error> {
        self.inner.get_identity_updates_v2(request).await
    }

    async fn get_inbox_ids(
        &self,
        request: identity_v1::GetInboxIdsRequest,
    ) -> Result<identity_v1::GetInboxIdsResponse, Self::Error> {
        self.inner.get_inbox_ids(request).await
    }

    async fn verify_smart_contract_wallet_signatures(
        &self,
        request: identity_v1::VerifySmartContractWalletSignaturesRequest,
    ) -> Result<identity_v1::VerifySmartContractWalletSignaturesResponse, Self::Error> {
        self.inner
            .verify_smart_contract_wallet_signatures(request)
            .await
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<C> XmtpMlsStreams for BoxedStreamsClient<C>
where
    C: Send + Sync + XmtpMlsStreams,
    C::GroupMessageStream: 'static,
    C::WelcomeMessageStream: 'static,
{
    type GroupMessageStream = xmtp_proto::api_client::BoxedGroupS<Self::Error>;
    type WelcomeMessageStream = xmtp_proto::api_client::BoxedWelcomeS<Self::Error>;
    type Error = <C as XmtpMlsStreams>::Error;

    async fn subscribe_group_messages(
        &self,
        group_ids: &[&GroupId],
    ) -> Result<Self::GroupMessageStream, Self::Error> {
        let s = self.inner.subscribe_group_messages(group_ids).await?;
        Ok(Box::pin(s) as Pin<Box<_>>)
    }

    async fn subscribe_welcome_messages(
        &self,
        installations: &[&InstallationId],
    ) -> Result<Self::WelcomeMessageStream, Self::Error> {
        let s = self.inner.subscribe_welcome_messages(installations).await?;
        Ok(Box::pin(s) as Pin<Box<_>>)
    }
}

impl<C> HasStats for BoxedStreamsClient<C>
where
    C: HasStats,
{
    fn aggregate_stats(&self) -> xmtp_proto::api_client::AggregateStats {
        self.inner.aggregate_stats()
    }

    fn mls_stats(&self) -> ApiStats {
        self.inner.mls_stats()
    }

    fn identity_stats(&self) -> IdentityStats {
        self.inner.identity_stats()
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<C> IsConnectedCheck for BoxedStreamsClient<C>
where
    C: IsConnectedCheck + Send + Sync,
{
    async fn is_connected(&self) -> bool {
        self.inner.is_connected().await
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<C: XmtpQuery> XmtpQuery for BoxedStreamsClient<C> {
    type Error = <C as XmtpQuery>::Error;

    async fn query_at(
        &self,
        topic: xmtp_proto::types::Topic,
        at: Option<xmtp_proto::types::GlobalCursor>,
    ) -> Result<crate::protocol::XmtpEnvelope, Self::Error> {
        <C as XmtpQuery>::query_at(&self.inner, topic, at).await
    }
}

impl<A: CursorAwareApi> CursorAwareApi for BoxedStreamsClient<A> {
    type CursorStore = A::CursorStore;

    fn set_cursor_store(&self, store: Self::CursorStore) {
        <A as CursorAwareApi>::set_cursor_store(&self.inner, store);
    }
}
