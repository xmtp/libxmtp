//! Client streams: streamed welcome processing, conversation, message, consent and
//! preference streams, and catch-up.

use super::*;

impl<Context> Client<Context>
where
    Context: XmtpSharedContext + 'static,
{
    /// Use push metadata as a target. Fetch its complete prefix before processing.
    pub async fn process_streamed_welcome_message(
        &self,
        envelope_bytes: Vec<u8>,
    ) -> Result<Vec<MlsGroup<Context>>, SubscribeError> {
        subscriptions::process_streamed_welcome_message(&self.context, envelope_bytes).await
    }

    pub async fn stream_conversations(
        &self,
        conversation_type: Option<ConversationType>,
        include_duplicate_dms: bool,
    ) -> Result<
        impl Stream<Item = Result<MlsGroup<Context>, SubscribeError>> + use<'_, Context>,
        SubscribeError,
    >
    where
        Context::ApiClient: XmtpMlsStreams,
    {
        subscriptions::stream_conversations(&self.context, conversation_type, include_duplicate_dms)
            .await
    }

    /// Stream conversations but decouple the lifetime of 'self' from the stream.
    pub async fn stream_conversations_owned(
        &self,
        conversation_type: Option<ConversationType>,
        include_duplicate_dms: bool,
    ) -> Result<
        impl Stream<Item = Result<MlsGroup<Context>, SubscribeError>> + 'static + use<Context>,
        SubscribeError,
    >
    where
        Context::ApiClient: XmtpMlsStreams,
    {
        subscriptions::stream_conversations_owned(
            self.context.clone(),
            conversation_type,
            include_duplicate_dms,
        )
        .await
    }
}

impl<Context> Client<Context>
where
    Context: XmtpSharedContext + 'static,
    Context::ApiClient: XmtpMlsStreams + 'static,
    Context::MlsStorage: 'static,
{
    pub fn stream_conversations_with_callback(
        client: Arc<Client<Context>>,
        conversation_type: Option<ConversationType>,
        convo_callback: impl FnMut(Result<MlsGroup<Context>, SubscribeError>) + MaybeSend + 'static,
        on_close: impl FnOnce() + MaybeSend + 'static,
        include_duplicate_dms: bool,
    ) -> impl StreamHandle<StreamOutput = Result<(), SubscribeError>> {
        subscriptions::stream_conversations_with_callback(
            client.context.clone(),
            client,
            conversation_type,
            convo_callback,
            on_close,
            include_duplicate_dms,
        )
    }

    pub async fn stream_all_messages(
        &self,
        conversation_type: Option<ConversationType>,
        consent_state: Option<Vec<ConsentState>>,
    ) -> Result<impl Stream<Item = Result<StoredGroupMessage, SubscribeError>> + '_, SubscribeError>
    {
        subscriptions::stream_all_messages(&self.context, conversation_type, consent_state).await
    }

    pub async fn stream_all_messages_owned(
        &self,
        conversation_type: Option<ConversationType>,
        consent_state: Option<Vec<ConsentState>>,
    ) -> Result<
        impl Stream<Item = Result<StoredGroupMessage, SubscribeError>> + 'static + use<Context>,
        SubscribeError,
    > {
        subscriptions::stream_all_messages_owned(
            self.context.clone(),
            conversation_type,
            consent_state,
        )
        .await
    }

    pub fn stream_consent_with_callback(
        client: Arc<Client<Context>>,
        callback: impl FnMut(Result<Vec<StoredConsentRecord>, SubscribeError>) + MaybeSend + 'static,
        on_close: impl FnOnce() + MaybeSend + 'static,
    ) -> impl StreamHandle<StreamOutput = Result<(), SubscribeError>> {
        subscriptions::stream_consent_with_callback(
            client.context.clone(),
            client,
            callback,
            on_close,
        )
    }

    pub fn stream_preferences_with_callback(
        client: Arc<Client<Context>>,
        callback: impl FnMut(Result<Vec<PreferenceUpdate>, SubscribeError>) + MaybeSend + 'static,
        on_close: impl FnOnce() + MaybeSend + 'static,
    ) -> impl StreamHandle<StreamOutput = Result<(), SubscribeError>> {
        subscriptions::stream_preferences_with_callback(
            client.context.clone(),
            client,
            callback,
            on_close,
        )
    }

    pub fn stream_message_deletions_with_callback(
        client: Arc<Client<Context>>,
        callback: impl FnMut(Result<DecodedMessage, SubscribeError>) + MaybeSend + 'static,
        on_close: impl FnOnce() + MaybeSend + 'static,
    ) -> impl StreamHandle<StreamOutput = Result<(), SubscribeError>> {
        subscriptions::stream_message_deletions_with_callback(
            client.context.clone(),
            client,
            callback,
            on_close,
        )
    }
}

impl<Context> Client<Context>
where
    Context: XmtpSharedContext + 'static,
    Context::ApiClient: XmtpMlsStreams + 'static,
    Context::MlsStorage: 'static,
    <Context::ApiClient as XmtpMlsStreams>::GroupMessageStream: Unpin,
    <Context::ApiClient as XmtpMlsStreams>::WelcomeMessageStream: Unpin,
{
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn stream_all_messages_owned_with_stats(
        &self,
        conversation_type: Option<ConversationType>,
        consent_state: Option<Vec<ConsentState>>,
    ) -> Result<
        impl StreamWithStats<Item = Result<StoredGroupMessage, SubscribeError>> + 'static,
        SubscribeError,
    > {
        subscriptions::stream_all_messages_owned_with_stats(
            self.context.clone(),
            conversation_type,
            consent_state,
        )
        .await
    }
}

impl<C: XmtpSharedContext + 'static> Client<C> {
    /// Process fixed starting targets and only the discoveries from the enrolled Welcomes.
    pub async fn catch_up_to_live(
        &self,
        timeout: Option<xmtp_common::time::Duration>,
    ) -> Result<CatchUpSummary, CatchUpError> {
        subscriptions::catch_up::catch_up_to_live(&self.context, timeout).await
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<C> Client<C>
where
    C: XmtpSharedContext + 'static,
    C::ApiClient:
        XmtpMlsBidiStreams + XmtpMlsStreams + ApiClientIdentity + Clone + Send + Sync + 'static,
    <C::ApiClient as XmtpMlsBidiStreams>::SubscribeStream: 'static,
{
    pub fn stream_all_messages_with_callback_dispatch(
        client: Arc<Client<C>>,
        conversation_type: Option<ConversationType>,
        consent_states: Option<Vec<ConsentState>>,
        callback: impl FnMut(Result<StoredGroupMessage, SubscribeError>) + MaybeSend + 'static,
        on_close: impl FnOnce() + MaybeSend + 'static,
    ) -> impl StreamHandle<StreamOutput = Result<(), SubscribeError>> {
        router_callbacks::stream_all_messages_with_callback_dispatch(
            client.context.clone(),
            client,
            conversation_type,
            consent_states,
            callback,
            on_close,
        )
    }

    pub fn stream_conversations_with_callback_dispatch(
        client: Arc<Client<C>>,
        conversation_type: Option<ConversationType>,
        include_duplicate_dms: bool,
        callback: impl FnMut(Result<MlsGroup<C>, SubscribeError>) + MaybeSend + 'static,
        on_close: impl FnOnce() + MaybeSend + 'static,
    ) -> impl StreamHandle<StreamOutput = Result<(), SubscribeError>> {
        router_callbacks::stream_conversations_with_callback_dispatch(
            client.context.clone(),
            client,
            conversation_type,
            include_duplicate_dms,
            callback,
            on_close,
        )
    }
}
