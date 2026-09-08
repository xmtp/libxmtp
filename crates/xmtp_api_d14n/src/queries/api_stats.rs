use xmtp_proto::{
    api::HasStats,
    api_client::{AggregateStats, ApiStats, IdentityStats, XmtpBackendClient, XmtpMlsStreams},
    backend_v1::*,
    types::{GroupId, InstallationId, TopicCursor},
};
#[derive(Clone, Debug)]
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
    pub fn inner(&self) -> &C {
        &self.inner
    }
}
#[xmtp_common::async_trait]
impl<C: XmtpBackendClient> XmtpBackendClient for TrackedStatsClient<C> {
    type Error = C::Error;
    async fn publish(&self, request: PublishRequest) -> Result<PublishResponse, Self::Error> {
        self.stats.publish.count_request();
        self.inner.publish(request).await
    }
    async fn query(&self, request: QueryRequest) -> Result<QueryResponse, Self::Error> {
        self.stats.query.count_request();
        self.inner.query(request).await
    }
    async fn query_newest(
        &self,
        request: QueryNewestRequest,
    ) -> Result<QueryNewestResponse, Self::Error> {
        self.stats.query_newest.count_request();
        self.inner.query_newest(request).await
    }
    async fn get(&self, request: GetRequest) -> Result<ServerEnvelope, Self::Error> {
        self.stats.get.count_request();
        self.inner.get(request).await
    }
    async fn get_inbox_ids(
        &self,
        request: GetInboxIdsRequest,
    ) -> Result<GetInboxIdsResponse, Self::Error> {
        self.identity_stats.get_inbox_ids.count_request();
        self.inner.get_inbox_ids(request).await
    }
    async fn verify_smart_contract_wallet_signatures(
        &self,
        request: VerifySmartContractWalletSignaturesRequest,
    ) -> Result<VerifySmartContractWalletSignaturesResponse, Self::Error> {
        self.identity_stats
            .verify_smart_contract_wallet_signatures
            .count_request();
        self.inner
            .verify_smart_contract_wallet_signatures(request)
            .await
    }
}
#[xmtp_common::async_trait]
impl<C: XmtpMlsStreams> XmtpMlsStreams for TrackedStatsClient<C> {
    type Error = C::Error;
    type GroupMessageStream = C::GroupMessageStream;
    type WelcomeMessageStream = C::WelcomeMessageStream;
    async fn subscribe_group_messages(
        &self,
        groups: &[&GroupId],
    ) -> Result<Self::GroupMessageStream, Self::Error> {
        self.stats.subscribe_static.count_request();
        self.inner.subscribe_group_messages(groups).await
    }
    async fn subscribe_group_messages_with_cursors(
        &self,
        groups: &TopicCursor,
    ) -> Result<Self::GroupMessageStream, Self::Error> {
        self.stats.subscribe_static.count_request();
        self.inner
            .subscribe_group_messages_with_cursors(groups)
            .await
    }
    async fn subscribe_welcome_messages(
        &self,
        installations: &[&InstallationId],
    ) -> Result<Self::WelcomeMessageStream, Self::Error> {
        self.stats.subscribe_static.count_request();
        self.inner.subscribe_welcome_messages(installations).await
    }
    async fn subscribe_welcome_messages_with_cursors(
        &self,
        installations: &TopicCursor,
    ) -> Result<Self::WelcomeMessageStream, Self::Error> {
        self.stats.subscribe_static.count_request();
        self.inner
            .subscribe_welcome_messages_with_cursors(installations)
            .await
    }
}
xmtp_common::if_native! {
    use xmtp_proto::api_client::XmtpMlsBidiStreams;

    // `XmtpMlsBidiStreams` is native-only, so this forward is gated like the
    // trait. It carries no per-call stat (the bidi stream is opened once and
    // mutated in place, not counted per RPC like the unary/stream calls above);
    // it exists so the stats wrapper is bidi-capable, letting the standard
    // feature-switched test client open a `BidiConnection`.
    #[xmtp_common::async_trait]
    impl<C> XmtpMlsBidiStreams for TrackedStatsClient<C>
    where
        C: XmtpMlsBidiStreams,
    {
        type SubscribeStream = <C as XmtpMlsBidiStreams>::SubscribeStream;
        type Error = <C as XmtpMlsBidiStreams>::Error;

        fn host(&self) -> &str {
            self.inner.host()
        }

        async fn subscribe_bidi(
            &self,
            requests: futures::stream::BoxStream<'static, xmtp_proto::mls_v1::SubscribeRequest>,
        ) -> Result<Self::SubscribeStream, Self::Error> {
            self.stats.subscribe.count_request();
            self.inner.subscribe_bidi(requests).await
        }
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

#[xmtp_common::async_trait]
impl<C: xmtp_proto::api::IsConnectedCheck> xmtp_proto::api::IsConnectedCheck
    for TrackedStatsClient<C>
{
    async fn is_connected(&self) -> bool {
        self.inner.is_connected().await
    }
}
