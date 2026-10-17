use super::*;
use futures::StreamExt;
use xmtp_proto::{
    api::{HasStats, IsConnectedCheck},
    api_client::*,
    types::*,
};

#[xmtp_common::async_trait]
impl<A: XmtpBackendClient> XmtpBackendClient for GuardedApi<A> {
    type Error = ApiError;
    async fn publish(&self, request: wire::PublishRequest) -> Result<wire::PublishResponse> {
        self.check_preflight().await?;
        self.raw.publish(request).await.map_err(dyn_err)
    }
    async fn create_upload(
        &self,
        request: wire::CreateUploadRequest,
    ) -> Result<wire::CreateUploadResponse> {
        self.check_preflight().await?;
        self.raw.create_upload(request).await.map_err(dyn_err)
    }
    async fn query(&self, request: wire::QueryRequest) -> Result<wire::QueryResponse> {
        self.check_preflight().await?;
        self.raw.query(request).await.map_err(dyn_err)
    }
    async fn query_newest(
        &self,
        request: wire::QueryNewestRequest,
    ) -> Result<wire::QueryNewestResponse> {
        self.check_preflight().await?;
        self.raw.query_newest(request).await.map_err(dyn_err)
    }
    async fn get_inbox_ids(
        &self,
        request: wire::GetInboxIdsRequest,
    ) -> Result<wire::GetInboxIdsResponse> {
        self.check_preflight().await?;
        self.raw.get_inbox_ids(request).await.map_err(dyn_err)
    }
    async fn get_configuration(
        &self,
        request: wire::GetConfigurationRequest,
    ) -> Result<wire::GetConfigurationResponse> {
        self.check_preflight().await?;
        self.raw.get_configuration(request).await.map_err(dyn_err)
    }
    async fn verify_smart_contract_wallet_signatures(
        &self,
        request: wire::VerifySmartContractWalletSignaturesRequest,
    ) -> Result<wire::VerifySmartContractWalletSignaturesResponse> {
        self.check_preflight().await?;
        self.raw
            .verify_smart_contract_wallet_signatures(request)
            .await
            .map_err(dyn_err)
    }
    async fn register(&self, request: wire::RegisterRequest) -> Result<wire::RecipientState> {
        self.check_preflight().await?;
        self.raw.register(request).await.map_err(dyn_err)
    }
    async fn unregister(
        &self,
        request: wire::UnregisterRequest,
    ) -> Result<wire::UnregisterResponse> {
        self.check_preflight().await?;
        self.raw.unregister(request).await.map_err(dyn_err)
    }
    async fn update_subscriptions(
        &self,
        request: wire::UpdateSubscriptionsRequest,
    ) -> Result<wire::RecipientState> {
        self.check_preflight().await?;
        self.raw
            .update_subscriptions(request)
            .await
            .map_err(dyn_err)
    }
    fn backend_url(&self) -> Option<&str> {
        self.raw.backend_url()
    }
    fn has_credential_source(&self) -> bool {
        self.raw.has_credential_source()
    }
    fn set_limits(&self, limits: Arc<xmtp_configuration::LimitsConfiguration>) {
        self.raw.set_limits(limits);
    }
    fn register_client_event_writer(&self, writer: &Arc<dyn xmtp_events::EventWriter<()>>) {
        self.raw.register_client_event_writer(writer);
    }
    fn unregister_client_event_writer(&self, writer: &Arc<dyn xmtp_events::EventWriter<()>>) {
        self.raw.unregister_client_event_writer(writer);
    }
}

#[xmtp_common::async_trait]
impl<A> XmtpMlsStreams for GuardedApi<A>
where
    A: XmtpMlsStreams + XmtpBackendClient,
    A::GroupMessageStream: 'static,
    A::WelcomeMessageStream: 'static,
{
    type Error = ApiError;
    type GroupMessageStream = xmtp_common::BoxDynStream<'static, Result<GroupMessage>>;
    type WelcomeMessageStream = xmtp_common::BoxDynStream<'static, Result<WelcomeMessage>>;
    async fn subscribe_envelopes_with_cursors(
        &self,
        cursors: &TopicCursor,
        limits: IncomingBatchLimits,
    ) -> Result<IncomingSubscription<ApiError>> {
        self.check_preflight().await?;
        self.raw
            .subscribe_envelopes_with_cursors(cursors, limits)
            .await
            .map(|stream| stream.map_error(dyn_err))
            .map_err(dyn_err)
    }
    async fn subscribe_group_messages(
        &self,
        group_ids: &[&GroupId],
    ) -> Result<Self::GroupMessageStream> {
        self.check_preflight().await?;
        Ok(Box::pin(
            self.raw
                .subscribe_group_messages(group_ids)
                .await
                .map_err(dyn_err)?
                .map(|item| item.map_err(dyn_err)),
        ))
    }
    async fn subscribe_group_messages_with_cursors(
        &self,
        cursors: &TopicCursor,
    ) -> Result<Self::GroupMessageStream> {
        self.check_preflight().await?;
        Ok(Box::pin(
            self.raw
                .subscribe_group_messages_with_cursors(cursors)
                .await
                .map_err(dyn_err)?
                .map(|item| item.map_err(dyn_err)),
        ))
    }
    async fn subscribe_welcome_messages(
        &self,
        installations: &[&InstallationId],
    ) -> Result<Self::WelcomeMessageStream> {
        self.check_preflight().await?;
        Ok(Box::pin(
            self.raw
                .subscribe_welcome_messages(installations)
                .await
                .map_err(dyn_err)?
                .map(|item| item.map_err(dyn_err)),
        ))
    }
    async fn subscribe_welcome_messages_with_cursors(
        &self,
        cursors: &TopicCursor,
    ) -> Result<Self::WelcomeMessageStream> {
        self.check_preflight().await?;
        Ok(Box::pin(
            self.raw
                .subscribe_welcome_messages_with_cursors(cursors)
                .await
                .map_err(dyn_err)?
                .map(|item| item.map_err(dyn_err)),
        ))
    }
}
xmtp_common::if_native! {
#[xmtp_common::async_trait]
impl<A> XmtpMlsBidiStreams for GuardedApi<A>
where A: XmtpMlsBidiStreams + XmtpBackendClient, A::SubscribeStream: 'static,
{
    type Error = ApiError;
    type SubscribeStream = xmtp_common::BoxDynStream<'static, Result<wire::SubscribeResponse>>;
    fn host(&self) -> &str { self.raw.host() }
    fn bidi_limits(&self) -> Arc<xmtp_configuration::LimitsConfiguration> { self.raw.bidi_limits() }
    async fn subscribe_bidi(&self, requests: futures::stream::BoxStream<'static, wire::SubscribeRequest>) -> Result<Self::SubscribeStream> {
        self.check_preflight().await?;
        Ok(Box::pin(self.raw.subscribe_bidi(requests).await.map_err(dyn_err)?.map(|item| item.map_err(dyn_err))))
    }
}
}
impl<A: HasStats> HasStats for GuardedApi<A> {
    fn aggregate_stats(&self) -> AggregateStats {
        self.raw.aggregate_stats()
    }
    fn mls_stats(&self) -> ApiStats {
        self.raw.mls_stats()
    }
    fn identity_stats(&self) -> IdentityStats {
        self.raw.identity_stats()
    }
}
#[xmtp_common::async_trait]
impl<A: IsConnectedCheck> IsConnectedCheck for GuardedApi<A> {
    async fn is_connected(&self) -> bool {
        self.raw.is_connected().await
    }
}
