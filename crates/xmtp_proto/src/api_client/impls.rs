use super::*;

#[xmtp_common::async_trait]
impl<T: XmtpBackendClient + ?Sized> XmtpBackendClient for Box<T> {
    type Error = T::Error;
    async fn publish(&self, request: PublishRequest) -> Result<PublishResponse, Self::Error> {
        (**self).publish(request).await
    }
    async fn query(&self, request: QueryRequest) -> Result<QueryResponse, Self::Error> {
        (**self).query(request).await
    }
    async fn query_newest(
        &self,
        request: QueryNewestRequest,
    ) -> Result<QueryNewestResponse, Self::Error> {
        (**self).query_newest(request).await
    }
    async fn get_inbox_ids(
        &self,
        request: GetInboxIdsRequest,
    ) -> Result<GetInboxIdsResponse, Self::Error> {
        (**self).get_inbox_ids(request).await
    }
    async fn get_configuration(
        &self,
        request: GetConfigurationRequest,
    ) -> Result<GetConfigurationResponse, Self::Error> {
        (**self).get_configuration(request).await
    }
    fn backend_url(&self) -> Option<&str> {
        (**self).backend_url()
    }

    fn has_credential_source(&self) -> bool {
        (**self).has_credential_source()
    }

    fn set_limits(&self, limits: std::sync::Arc<xmtp_configuration::LimitsConfiguration>) {
        (**self).set_limits(limits)
    }
    async fn verify_smart_contract_wallet_signatures(
        &self,
        request: VerifySmartContractWalletSignaturesRequest,
    ) -> Result<VerifySmartContractWalletSignaturesResponse, Self::Error> {
        (**self)
            .verify_smart_contract_wallet_signatures(request)
            .await
    }
    async fn register(&self, request: RegisterRequest) -> Result<RecipientState, Self::Error> {
        (**self).register(request).await
    }
    async fn unregister(
        &self,
        request: UnregisterRequest,
    ) -> Result<UnregisterResponse, Self::Error> {
        (**self).unregister(request).await
    }
    async fn update_subscriptions(
        &self,
        request: UpdateSubscriptionsRequest,
    ) -> Result<RecipientState, Self::Error> {
        (**self).update_subscriptions(request).await
    }
}

#[xmtp_common::async_trait]
impl<T: XmtpMlsStreams + ?Sized> XmtpMlsStreams for Box<T> {
    type Error = T::Error;
    type GroupMessageStream = T::GroupMessageStream;
    type WelcomeMessageStream = T::WelcomeMessageStream;
    async fn subscribe_envelopes_with_cursors(
        &self,
        cursors: &TopicCursor,
        limits: IncomingBatchLimits,
    ) -> Result<IncomingSubscription<Self::Error>, Self::Error> {
        (**self)
            .subscribe_envelopes_with_cursors(cursors, limits)
            .await
    }
    async fn subscribe_group_messages(
        &self,
        group_ids: &[&GroupId],
    ) -> Result<Self::GroupMessageStream, Self::Error> {
        (**self).subscribe_group_messages(group_ids).await
    }
    async fn subscribe_group_messages_with_cursors(
        &self,
        cursors: &TopicCursor,
    ) -> Result<Self::GroupMessageStream, Self::Error> {
        (**self)
            .subscribe_group_messages_with_cursors(cursors)
            .await
    }
    async fn subscribe_welcome_messages(
        &self,
        installations: &[&InstallationId],
    ) -> Result<Self::WelcomeMessageStream, Self::Error> {
        (**self).subscribe_welcome_messages(installations).await
    }
    async fn subscribe_welcome_messages_with_cursors(
        &self,
        cursors: &TopicCursor,
    ) -> Result<Self::WelcomeMessageStream, Self::Error> {
        (**self)
            .subscribe_welcome_messages_with_cursors(cursors)
            .await
    }
}

#[xmtp_common::async_trait]
impl<T: XmtpBackendClient + ?Sized> XmtpBackendClient for Arc<T> {
    type Error = T::Error;
    async fn publish(&self, request: PublishRequest) -> Result<PublishResponse, Self::Error> {
        (**self).publish(request).await
    }
    async fn query(&self, request: QueryRequest) -> Result<QueryResponse, Self::Error> {
        (**self).query(request).await
    }
    async fn query_newest(
        &self,
        request: QueryNewestRequest,
    ) -> Result<QueryNewestResponse, Self::Error> {
        (**self).query_newest(request).await
    }
    async fn get_inbox_ids(
        &self,
        request: GetInboxIdsRequest,
    ) -> Result<GetInboxIdsResponse, Self::Error> {
        (**self).get_inbox_ids(request).await
    }
    async fn get_configuration(
        &self,
        request: GetConfigurationRequest,
    ) -> Result<GetConfigurationResponse, Self::Error> {
        (**self).get_configuration(request).await
    }
    fn backend_url(&self) -> Option<&str> {
        (**self).backend_url()
    }

    fn has_credential_source(&self) -> bool {
        (**self).has_credential_source()
    }

    fn set_limits(&self, limits: std::sync::Arc<xmtp_configuration::LimitsConfiguration>) {
        (**self).set_limits(limits)
    }
    async fn verify_smart_contract_wallet_signatures(
        &self,
        request: VerifySmartContractWalletSignaturesRequest,
    ) -> Result<VerifySmartContractWalletSignaturesResponse, Self::Error> {
        (**self)
            .verify_smart_contract_wallet_signatures(request)
            .await
    }
    async fn register(&self, request: RegisterRequest) -> Result<RecipientState, Self::Error> {
        (**self).register(request).await
    }
    async fn unregister(
        &self,
        request: UnregisterRequest,
    ) -> Result<UnregisterResponse, Self::Error> {
        (**self).unregister(request).await
    }
    async fn update_subscriptions(
        &self,
        request: UpdateSubscriptionsRequest,
    ) -> Result<RecipientState, Self::Error> {
        (**self).update_subscriptions(request).await
    }
}

#[xmtp_common::async_trait]
impl<T: XmtpMlsStreams + ?Sized> XmtpMlsStreams for Arc<T> {
    type Error = T::Error;
    type GroupMessageStream = T::GroupMessageStream;
    type WelcomeMessageStream = T::WelcomeMessageStream;
    async fn subscribe_envelopes_with_cursors(
        &self,
        cursors: &TopicCursor,
        limits: IncomingBatchLimits,
    ) -> Result<IncomingSubscription<Self::Error>, Self::Error> {
        (**self)
            .subscribe_envelopes_with_cursors(cursors, limits)
            .await
    }
    async fn subscribe_group_messages(
        &self,
        group_ids: &[&GroupId],
    ) -> Result<Self::GroupMessageStream, Self::Error> {
        (**self).subscribe_group_messages(group_ids).await
    }
    async fn subscribe_group_messages_with_cursors(
        &self,
        cursors: &TopicCursor,
    ) -> Result<Self::GroupMessageStream, Self::Error> {
        (**self)
            .subscribe_group_messages_with_cursors(cursors)
            .await
    }
    async fn subscribe_welcome_messages(
        &self,
        installations: &[&InstallationId],
    ) -> Result<Self::WelcomeMessageStream, Self::Error> {
        (**self).subscribe_welcome_messages(installations).await
    }
    async fn subscribe_welcome_messages_with_cursors(
        &self,
        cursors: &TopicCursor,
    ) -> Result<Self::WelcomeMessageStream, Self::Error> {
        (**self)
            .subscribe_welcome_messages_with_cursors(cursors)
            .await
    }
}

#[xmtp_common::async_trait]
impl<T: XmtpBackendClient + ?Sized> XmtpBackendClient for &T {
    type Error = T::Error;
    async fn publish(&self, request: PublishRequest) -> Result<PublishResponse, Self::Error> {
        (**self).publish(request).await
    }
    async fn query(&self, request: QueryRequest) -> Result<QueryResponse, Self::Error> {
        (**self).query(request).await
    }
    async fn query_newest(
        &self,
        request: QueryNewestRequest,
    ) -> Result<QueryNewestResponse, Self::Error> {
        (**self).query_newest(request).await
    }
    async fn get_inbox_ids(
        &self,
        request: GetInboxIdsRequest,
    ) -> Result<GetInboxIdsResponse, Self::Error> {
        (**self).get_inbox_ids(request).await
    }
    async fn get_configuration(
        &self,
        request: GetConfigurationRequest,
    ) -> Result<GetConfigurationResponse, Self::Error> {
        (**self).get_configuration(request).await
    }
    fn backend_url(&self) -> Option<&str> {
        (**self).backend_url()
    }

    fn has_credential_source(&self) -> bool {
        (**self).has_credential_source()
    }

    fn set_limits(&self, limits: std::sync::Arc<xmtp_configuration::LimitsConfiguration>) {
        (**self).set_limits(limits)
    }
    async fn verify_smart_contract_wallet_signatures(
        &self,
        request: VerifySmartContractWalletSignaturesRequest,
    ) -> Result<VerifySmartContractWalletSignaturesResponse, Self::Error> {
        (**self)
            .verify_smart_contract_wallet_signatures(request)
            .await
    }
    async fn register(&self, request: RegisterRequest) -> Result<RecipientState, Self::Error> {
        (**self).register(request).await
    }
    async fn unregister(
        &self,
        request: UnregisterRequest,
    ) -> Result<UnregisterResponse, Self::Error> {
        (**self).unregister(request).await
    }
    async fn update_subscriptions(
        &self,
        request: UpdateSubscriptionsRequest,
    ) -> Result<RecipientState, Self::Error> {
        (**self).update_subscriptions(request).await
    }
}

#[xmtp_common::async_trait]
impl<T: XmtpMlsStreams + ?Sized> XmtpMlsStreams for &T {
    type Error = T::Error;
    type GroupMessageStream = T::GroupMessageStream;
    type WelcomeMessageStream = T::WelcomeMessageStream;
    async fn subscribe_envelopes_with_cursors(
        &self,
        cursors: &TopicCursor,
        limits: IncomingBatchLimits,
    ) -> Result<IncomingSubscription<Self::Error>, Self::Error> {
        (**self)
            .subscribe_envelopes_with_cursors(cursors, limits)
            .await
    }
    async fn subscribe_group_messages(
        &self,
        group_ids: &[&GroupId],
    ) -> Result<Self::GroupMessageStream, Self::Error> {
        (**self).subscribe_group_messages(group_ids).await
    }
    async fn subscribe_group_messages_with_cursors(
        &self,
        cursors: &TopicCursor,
    ) -> Result<Self::GroupMessageStream, Self::Error> {
        (**self)
            .subscribe_group_messages_with_cursors(cursors)
            .await
    }
    async fn subscribe_welcome_messages(
        &self,
        installations: &[&InstallationId],
    ) -> Result<Self::WelcomeMessageStream, Self::Error> {
        (**self).subscribe_welcome_messages(installations).await
    }
    async fn subscribe_welcome_messages_with_cursors(
        &self,
        cursors: &TopicCursor,
    ) -> Result<Self::WelcomeMessageStream, Self::Error> {
        (**self)
            .subscribe_welcome_messages_with_cursors(cursors)
            .await
    }
}

xmtp_common::if_native! {
    // `XmtpMlsBidiStreams` is native-only (see `api_client.rs`), so its boxed /
    // arced forwarders are gated the same way. Without these, erasing a client
    // into `Box<dyn XmtpMlsBidiStreams>` / `Arc<dyn XmtpMlsBidiStreams>` would
    // fail to compile on `subscribe_bidi`, unlike every other client trait here.
    #[xmtp_common::async_trait]
    impl<T> XmtpMlsBidiStreams for Box<T>
    where
        T: XmtpMlsBidiStreams + Sync + ?Sized,
    {
        type Error = <T as XmtpMlsBidiStreams>::Error;
        type SubscribeStream = <T as XmtpMlsBidiStreams>::SubscribeStream;

        fn host(&self) -> &str {
            (**self).host()
        }

        fn bidi_limits(&self) -> std::sync::Arc<xmtp_configuration::LimitsConfiguration> {
            (**self).bidi_limits()
        }

        async fn subscribe_bidi(
            &self,
            requests: futures::stream::BoxStream<'static, crate::backend_v1::SubscribeRequest>,
        ) -> Result<Self::SubscribeStream, Self::Error> {
            (**self).subscribe_bidi(requests).await
        }
    }

    #[xmtp_common::async_trait]
    impl<T> XmtpMlsBidiStreams for Arc<T>
    where
        T: XmtpMlsBidiStreams + ?Sized,
    {
        type Error = <T as XmtpMlsBidiStreams>::Error;
        type SubscribeStream = <T as XmtpMlsBidiStreams>::SubscribeStream;

        fn host(&self) -> &str {
            (**self).host()
        }

        fn bidi_limits(&self) -> std::sync::Arc<xmtp_configuration::LimitsConfiguration> {
            (**self).bidi_limits()
        }

        async fn subscribe_bidi(
            &self,
            requests: futures::stream::BoxStream<'static, crate::backend_v1::SubscribeRequest>,
        ) -> Result<Self::SubscribeStream, Self::Error> {
            (**self).subscribe_bidi(requests).await
        }
    }
}
