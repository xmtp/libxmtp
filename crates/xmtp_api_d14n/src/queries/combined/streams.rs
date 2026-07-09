use crate::MigrationClient;
use crate::protocol::CursorStore;

use xmtp_proto::api::{ApiClientError, Client};
use xmtp_proto::api_client::{BoxedGroupS, BoxedWelcomeS, XmtpMlsStreams};
use xmtp_proto::types::{GroupId, InstallationId, TopicCursor};

#[xmtp_common::async_trait]
impl<V3, D14n, Store> XmtpMlsStreams for MigrationClient<V3, D14n, Store>
where
    V3: Client,
    D14n: Client,
    Store: CursorStore + Clone,
{
    type Error = ApiClientError;

    type GroupMessageStream = BoxedGroupS<ApiClientError>;

    type WelcomeMessageStream = BoxedWelcomeS<ApiClientError>;

    async fn subscribe_group_messages(
        &self,
        group_ids: &[&GroupId],
    ) -> Result<Self::GroupMessageStream, Self::Error> {
        Ok(self
            .choose_client()
            .await?
            .subscribe_group_messages(group_ids)
            .await?)
    }

    async fn subscribe_group_messages_with_cursors(
        &self,
        topics: &TopicCursor,
    ) -> Result<Self::GroupMessageStream, Self::Error> {
        Ok(self
            .choose_client()
            .await?
            .subscribe_group_messages_with_cursors(topics)
            .await?)
    }

    async fn subscribe_welcome_messages(
        &self,
        installations: &[&InstallationId],
    ) -> Result<Self::WelcomeMessageStream, Self::Error> {
        Ok(self
            .choose_client()
            .await?
            .subscribe_welcome_messages(installations)
            .await?)
    }
}

xmtp_common::if_native! {
    // The v3-shaped XIP-83 bidi surface (`mls_v1` frames). This client cannot
    // serve it — the d14n wire speaks its own envelope binding — so it refuses
    // at runtime with an unretryable error. The dyn full API always *has*
    // `subscribe_bidi`; support is a runtime property of the backend. There is
    // no automatic fallback to the legacy streams yet (that latch is follow-on
    // work), so the opt-in env gate must only be enabled against v3 backends.
    #[xmtp_common::async_trait]
    impl<V3, D14n, Store> xmtp_proto::api_client::XmtpMlsBidiStreams for MigrationClient<V3, D14n, Store>
    where
        V3: Client,
        D14n: Client,
        Store: CursorStore + Clone,
    {
        type SubscribeStream = xmtp_proto::api_client::BoxedSubscribeS<ApiClientError>;
        type Error = ApiClientError;

        async fn subscribe_bidi(
            &self,
            _requests: futures::stream::BoxStream<'static, xmtp_proto::mls_v1::SubscribeRequest>,
        ) -> Result<Self::SubscribeStream, Self::Error> {
            Err(ApiClientError::OtherUnretryable(
                "the v3 bidi subscription is not available on this client".into(),
            ))
        }
    }
}
