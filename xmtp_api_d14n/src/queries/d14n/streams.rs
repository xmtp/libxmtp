use crate::TryExtractorStream;
use crate::d14n::SubscribeEnvelopes;
use crate::protocol::{CursorStore, GroupMessageExtractor, WelcomeMessageExtractor};
use crate::queries::stream;

use super::D14nClient;
use xmtp_common::{MaybeSend, RetryableError};
use xmtp_proto::api::{ApiClientError, Client, QueryStream, XmtpStream};
use xmtp_proto::api_client::XmtpMlsStreams;
use xmtp_proto::types::{GroupId, InstallationId, TopicKind};
use xmtp_proto::xmtp::xmtpv4::message_api::SubscribeEnvelopesResponse;

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<C, Store, E> XmtpMlsStreams for D14nClient<C, Store>
where
    C: Send + Sync + Client<Error = E>,
    <C as Client>::Stream: MaybeSend + 'static,
    E: std::error::Error + RetryableError + Send + Sync + 'static,
    Store: CursorStore,
{
    type Error = ApiClientError<E>;

    type GroupMessageStream = TryExtractorStream<
        XmtpStream<<C as Client>::Stream, SubscribeEnvelopesResponse>,
        GroupMessageExtractor,
    >;

    type WelcomeMessageStream = TryExtractorStream<
        XmtpStream<<C as Client>::Stream, SubscribeEnvelopesResponse>,
        WelcomeMessageExtractor,
    >;

    async fn subscribe_group_messages(
        &self,
        group_ids: &[&GroupId],
    ) -> Result<Self::GroupMessageStream, Self::Error> {
        let topics = group_ids
            .iter()
            .map(|gid| TopicKind::GroupMessagesV1.create(gid))
            .collect::<Vec<_>>();
        let lcc = self
            .cursor_store
            .lcc_maybe_missing(&topics.iter().collect::<Vec<_>>())?;
        tracing::info!("subscribing to messages @cursor={}", lcc);
        let s = SubscribeEnvelopes::builder()
            .topics(topics)
            .last_seen(lcc)
            .build()?
            .stream(&self.client)
            .await?;
        Ok(stream::try_extractor(s))
    }

    async fn subscribe_welcome_messages(
        &self,
        installations: &[&InstallationId],
    ) -> Result<Self::WelcomeMessageStream, Self::Error> {
        let topics = installations
            .iter()
            .map(|ins| TopicKind::WelcomeMessagesV1.create(ins))
            .collect::<Vec<_>>();
        let lcc = self
            .cursor_store
            .lowest_common_cursor(&topics.iter().collect::<Vec<_>>())?;
        let s = SubscribeEnvelopes::builder()
            .topics(topics)
            .last_seen(lcc)
            .build()?
            .stream(&self.client)
            .await?;
        Ok(stream::try_extractor(s))
    }
}
