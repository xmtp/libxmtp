use crate::d14n::SubscribeEnvelopes;
use crate::protocol::{CursorStore, GroupMessageExtractor, WelcomeMessageExtractor};
use crate::queries::stream;
use crate::{FlattenedStream, OrderedStream, TryExtractorStream};

use super::D14nClient;
use xmtp_common::RetryableError;
use xmtp_proto::api::{ApiClientError, Client, QueryStream, XmtpStream};
use xmtp_proto::api_client::{Paged, XmtpMlsStreams};
use xmtp_proto::types::{GroupId, InstallationId, TopicCursor, TopicKind};
use xmtp_proto::xmtp::xmtpv4::message_api::SubscribeEnvelopesResponse;

type PagedItem = <SubscribeEnvelopesResponse as Paged>::Message;

type OrderedStreamT<C, Store> = OrderedStream<
    FlattenedStream<XmtpStream<<C as Client>::Stream, SubscribeEnvelopesResponse>>,
    Store,
    PagedItem,
>;

#[xmtp_common::async_trait]
impl<C, Store, E> XmtpMlsStreams for D14nClient<C, Store>
where
    C: Client<Error = E>,
    <C as Client>::Stream: 'static,
    E: RetryableError + 'static,
    Store: CursorStore + Clone,
{
    type Error = ApiClientError<E>;

    type GroupMessageStream = TryExtractorStream<OrderedStreamT<C, Store>, GroupMessageExtractor>;

    type WelcomeMessageStream = TryExtractorStream<
        XmtpStream<<C as Client>::Stream, SubscribeEnvelopesResponse>,
        WelcomeMessageExtractor,
    >;

    async fn subscribe_group_messages(
        &self,
        group_ids: &[&GroupId],
    ) -> Result<Self::GroupMessageStream, Self::Error> {
        if group_ids.is_empty() {
            let s = SubscribeEnvelopes::builder()
                .build()?
                .fake_stream(&self.client);
            let s = stream::ordered(
                stream::flattened(s),
                self.cursor_store.clone(),
                TopicCursor::default(),
            );
            return Ok(stream::try_extractor(s));
        }
        let topics = group_ids
            .iter()
            .map(|gid| TopicKind::GroupMessagesV1.create(gid))
            .collect::<Vec<_>>();
        let lcc = self
            .cursor_store
            .lcc_maybe_missing(&topics.iter().collect::<Vec<_>>())?;
        let topic_cursor: TopicCursor = self
            .cursor_store
            .latest_for_topics(&mut topics.iter())?
            .into();
        tracing::debug!("subscribing to messages @cursor={}", lcc);
        let s = SubscribeEnvelopes::builder()
            .topics(topics)
            .last_seen(lcc)
            .build()?
            .stream(&self.client)
            .await?;
        let s = stream::ordered(
            stream::flattened(s),
            self.cursor_store.clone(),
            topic_cursor,
        );
        Ok(stream::try_extractor(s))
    }

    async fn subscribe_group_messages_with_cursors(
        &self,
        topics: &TopicCursor,
    ) -> Result<Self::GroupMessageStream, Self::Error> {
        if topics.is_empty() {
            let s = SubscribeEnvelopes::builder()
                .build()?
                .fake_stream(&self.client);
            let s = stream::ordered(
                stream::flattened(s),
                self.cursor_store.clone(),
                TopicCursor::default(),
            );
            return Ok(stream::try_extractor(s));
        }
        // Compute the lowest common cursor from the provided cursors
        let lcc = topics.lcc();
        tracing::debug!(
            "subscribing to messages with provided cursors @cursor={}",
            lcc
        );
        let s = SubscribeEnvelopes::builder()
            .topics(topics.topics())
            .last_seen(lcc)
            .build()?
            .stream(&self.client)
            .await?;
        let s = stream::ordered(
            stream::flattened(s),
            self.cursor_store.clone(),
            topics.clone(),
        );
        Ok(stream::try_extractor(s))
    }

    async fn subscribe_welcome_messages(
        &self,
        installations: &[&InstallationId],
    ) -> Result<Self::WelcomeMessageStream, Self::Error> {
        if installations.is_empty() {
            let s = SubscribeEnvelopes::builder()
                .build()?
                .fake_stream(&self.client);
            return Ok(stream::try_extractor(s));
        }
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
