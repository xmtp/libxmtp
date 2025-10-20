use xmtp_proto::api::HasStats;
use xmtp_proto::api_client::AggregateStats;
use xmtp_proto::api_client::ApiStats;
use xmtp_proto::api_client::CursorAwareApi;
use xmtp_proto::api_client::IdentityStats;
use xmtp_proto::api_client::XmtpMlsClient;
use xmtp_proto::identity_v1;
use xmtp_proto::mls_v1;
use xmtp_proto::prelude::ApiBuilder;
use xmtp_proto::prelude::XmtpIdentityClient;
use xmtp_proto::prelude::XmtpMlsStreams;
use xmtp_proto::types::AppVersion;
use xmtp_proto::types::InstallationId;
use xmtp_proto::types::WelcomeMessage;
use xmtp_proto::types::{GroupId, GroupMessage};

use crate::protocol::XmtpQuery;

/// Wraps an ApiClient that tracks stats of each api call
#[derive(Clone)]
pub struct TrackedStatsClient<C> {
    inner: C,
    stats: ApiStats,
    identity_stats: IdentityStats,
}

impl<C> TrackedStatsClient<C> {
    pub fn new(inner: C) -> Self {
        Self {
            inner,
            stats: Default::default(),
            identity_stats: Default::default(),
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<C> XmtpMlsClient for TrackedStatsClient<C>
where
    C: Send + Sync + XmtpMlsClient,
{
    type Error = <C as XmtpMlsClient>::Error;

    async fn upload_key_package(
        &self,
        request: mls_v1::UploadKeyPackageRequest,
    ) -> Result<(), Self::Error> {
        self.stats.upload_key_package.count_request();
        self.inner.upload_key_package(request).await
    }

    async fn fetch_key_packages(
        &self,
        request: mls_v1::FetchKeyPackagesRequest,
    ) -> Result<mls_v1::FetchKeyPackagesResponse, Self::Error> {
        self.stats.fetch_key_package.count_request();
        self.inner.fetch_key_packages(request).await
    }

    async fn send_group_messages(
        &self,
        request: mls_v1::SendGroupMessagesRequest,
    ) -> Result<(), Self::Error> {
        self.stats.send_group_messages.count_request();
        self.inner.send_group_messages(request).await
    }

    async fn send_welcome_messages(
        &self,
        request: mls_v1::SendWelcomeMessagesRequest,
    ) -> Result<(), Self::Error> {
        self.stats.send_welcome_messages.count_request();
        self.inner.send_welcome_messages(request).await
    }
    async fn query_group_messages(
        &self,
        group_id: GroupId,
    ) -> Result<Vec<GroupMessage>, Self::Error> {
        self.stats.query_group_messages.count_request();
        self.inner.query_group_messages(group_id).await
    }

    async fn query_latest_group_message(
        &self,
        group_id: GroupId,
    ) -> Result<Option<GroupMessage>, Self::Error> {
        self.stats.query_group_messages.count_request();
        self.inner.query_latest_group_message(group_id).await
    }

    async fn query_welcome_messages(
        &self,
        installation_key: InstallationId,
    ) -> Result<Vec<WelcomeMessage>, Self::Error> {
        self.stats.query_welcome_messages.count_request();
        self.inner.query_welcome_messages(installation_key).await
    }

    async fn publish_commit_log(
        &self,
        request: mls_v1::BatchPublishCommitLogRequest,
    ) -> Result<(), Self::Error> {
        self.stats.publish_commit_log.count_request();
        self.inner.publish_commit_log(request).await
    }

    async fn query_commit_log(
        &self,
        request: mls_v1::BatchQueryCommitLogRequest,
    ) -> Result<mls_v1::BatchQueryCommitLogResponse, Self::Error> {
        self.stats.query_commit_log.count_request();
        self.inner.query_commit_log(request).await
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<C> XmtpIdentityClient for TrackedStatsClient<C>
where
    C: Send + Sync + XmtpIdentityClient,
{
    type Error = <C as XmtpIdentityClient>::Error;

    async fn publish_identity_update(
        &self,
        request: identity_v1::PublishIdentityUpdateRequest,
    ) -> Result<identity_v1::PublishIdentityUpdateResponse, Self::Error> {
        self.identity_stats.publish_identity_update.count_request();
        self.inner.publish_identity_update(request).await
    }

    async fn get_identity_updates_v2(
        &self,
        request: identity_v1::GetIdentityUpdatesRequest,
    ) -> Result<identity_v1::GetIdentityUpdatesResponse, Self::Error> {
        self.identity_stats.get_identity_updates_v2.count_request();
        self.inner.get_identity_updates_v2(request).await
    }

    async fn get_inbox_ids(
        &self,
        request: identity_v1::GetInboxIdsRequest,
    ) -> Result<identity_v1::GetInboxIdsResponse, Self::Error> {
        self.identity_stats.get_inbox_ids.count_request();
        self.inner.get_inbox_ids(request).await
    }

    async fn verify_smart_contract_wallet_signatures(
        &self,
        request: identity_v1::VerifySmartContractWalletSignaturesRequest,
    ) -> Result<identity_v1::VerifySmartContractWalletSignaturesResponse, Self::Error> {
        self.identity_stats
            .verify_smart_contract_wallet_signature
            .count_request();
        self.inner
            .verify_smart_contract_wallet_signatures(request)
            .await
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<C> XmtpMlsStreams for TrackedStatsClient<C>
where
    C: Send + Sync + XmtpMlsStreams,
{
    type GroupMessageStream = <C as XmtpMlsStreams>::GroupMessageStream;
    type WelcomeMessageStream = <C as XmtpMlsStreams>::WelcomeMessageStream;
    type Error = <C as XmtpMlsStreams>::Error;

    async fn subscribe_group_messages(
        &self,
        group_ids: &[&GroupId],
    ) -> Result<Self::GroupMessageStream, Self::Error> {
        self.stats.subscribe_messages.count_request();
        self.inner.subscribe_group_messages(group_ids).await
    }

    async fn subscribe_welcome_messages(
        &self,
        installations: &[&InstallationId],
    ) -> Result<Self::WelcomeMessageStream, Self::Error> {
        self.stats.subscribe_welcomes.count_request();
        self.inner.subscribe_welcome_messages(installations).await
    }
}

impl<C> HasStats for TrackedStatsClient<C> {
    fn aggregate_stats(&self) -> AggregateStats {
        AggregateStats {
            identity: self.identity_stats.clone(),
            mls: self.stats.clone(),
        }
    }

    fn mls_stats(&self) -> ApiStats {
        self.stats.clone()
    }

    fn identity_stats(&self) -> IdentityStats {
        self.identity_stats.clone()
    }
}

pub struct StatsBuilder<Builder> {
    client: Builder,
}

impl<Builder> StatsBuilder<Builder> {
    pub fn new(client: Builder) -> Self {
        Self { client }
    }
}

impl<C> TrackedStatsClient<C> {
    pub fn builder<T: Default>() -> StatsBuilder<T> {
        StatsBuilder::new(T::default())
    }
}

impl<Builder> ApiBuilder for StatsBuilder<Builder>
where
    Builder: ApiBuilder,
{
    type Output = TrackedStatsClient<<Builder as ApiBuilder>::Output>;

    type Error = <Builder as ApiBuilder>::Error;

    fn set_libxmtp_version(&mut self, version: String) -> Result<(), Self::Error> {
        <Builder as ApiBuilder>::set_libxmtp_version(&mut self.client, version)
    }

    fn set_app_version(&mut self, version: AppVersion) -> Result<(), Self::Error> {
        <Builder as ApiBuilder>::set_app_version(&mut self.client, version)
    }

    fn set_host(&mut self, host: String) {
        <Builder as ApiBuilder>::set_host(&mut self.client, host)
    }

    fn set_tls(&mut self, tls: bool) {
        <Builder as ApiBuilder>::set_tls(&mut self.client, tls)
    }

    fn rate_per_minute(&mut self, limit: u32) {
        <Builder as ApiBuilder>::rate_per_minute(&mut self.client, limit)
    }

    fn port(&self) -> Result<Option<String>, Self::Error> {
        <Builder as ApiBuilder>::port(&self.client)
    }

    fn host(&self) -> Option<&str> {
        <Builder as ApiBuilder>::host(&self.client)
    }

    fn build(self) -> Result<Self::Output, Self::Error> {
        Ok(TrackedStatsClient::new(<Builder as ApiBuilder>::build(
            self.client,
        )?))
    }

    fn set_retry(&mut self, retry: xmtp_common::Retry) {
        <Builder as ApiBuilder>::set_retry(&mut self.client, retry)
    }
}

impl<C> CursorAwareApi for TrackedStatsClient<C>
where
    C: CursorAwareApi,
{
    type CursorStore = <C as CursorAwareApi>::CursorStore;

    fn set_cursor_store(&self, store: Self::CursorStore) {
        <C as CursorAwareApi>::set_cursor_store(&self.inner, store);
    }
}

impl<Builder> CursorAwareApi for StatsBuilder<Builder>
where
    Builder: CursorAwareApi,
{
    type CursorStore = <Builder as CursorAwareApi>::CursorStore;

    fn set_cursor_store(&self, store: Self::CursorStore) {
        <Builder as CursorAwareApi>::set_cursor_store(&self.client, store);
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<C: XmtpQuery> XmtpQuery for TrackedStatsClient<C> {
    type Error = <C as XmtpQuery>::Error;

    async fn query_at(
        &self,
        topic: xmtp_proto::types::Topic,
        at: Option<xmtp_proto::types::GlobalCursor>,
    ) -> Result<crate::protocol::XmtpEnvelope, Self::Error> {
        <C as XmtpQuery>::query_at(&self.inner, topic, at).await
    }
}

#[cfg(any(test, feature = "test-utils"))]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use xmtp_configuration::LOCALHOST;
    use xmtp_proto::ToxicProxies;
    use xmtp_proto::{TestApiBuilder, prelude::XmtpTestClient};
    impl<C> XmtpTestClient for TrackedStatsClient<C>
    where
        C: XmtpTestClient,
    {
        type Builder = StatsBuilder<C::Builder>;
        fn create_local() -> Self::Builder {
            StatsBuilder::new(<C as XmtpTestClient>::create_local())
        }
        fn create_dev() -> Self::Builder {
            StatsBuilder::new(<C as XmtpTestClient>::create_dev())
        }
        fn create_gateway() -> Self::Builder {
            StatsBuilder::new(<C as XmtpTestClient>::create_gateway())
        }
        fn create_d14n() -> Self::Builder {
            StatsBuilder::new(<C as XmtpTestClient>::create_d14n())
        }
    }

    impl<Builder> TestApiBuilder for StatsBuilder<Builder>
    where
        Builder: ApiBuilder,
    {
        async fn with_toxiproxy(&mut self) -> ToxicProxies {
            let host = <Builder as ApiBuilder>::host(&self.client).unwrap();
            let proxies = xmtp_proto::init_toxi(&[host]).await;
            <Builder as ApiBuilder>::set_host(
                &mut self.client,
                format!("{LOCALHOST}:{}", proxies.ports()[0]),
            );
            proxies
        }
    }
}
