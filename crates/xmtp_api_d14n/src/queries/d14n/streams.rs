use crate::d14n::SubscribeTopics;
use crate::protocol::{CursorStore, GroupMessageExtractor, WelcomeMessageExtractor};
use crate::queries::stream;
use crate::{OrderedStream, StatusAwareStream, TryExtractorStream};

use super::D14nClient;
use xmtp_common::RetryableError;
use xmtp_proto::api::{ApiClientError, Client, QueryStream, XmtpStream};
use xmtp_proto::api_client::XmtpMlsStreams;
use xmtp_proto::types::{GroupId, InstallationId, TopicCursor, TopicKind};
use xmtp_proto::xmtp::xmtpv4::envelopes::OriginatorEnvelope;
use xmtp_proto::xmtp::xmtpv4::message_api::SubscribeTopicsResponse;

type StatusStreamT<C> = StatusAwareStream<
    XmtpStream<<C as Client>::Stream, SubscribeTopicsResponse>,
>;

type OrderedStreamT<C, Store> = OrderedStream<
    StatusStreamT<C>,
    Store,
    OriginatorEnvelope,
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
        StatusStreamT<C>,
        WelcomeMessageExtractor,
    >;

    async fn subscribe_group_messages(
        &self,
        group_ids: &[&GroupId],
    ) -> Result<Self::GroupMessageStream, Self::Error> {
        if group_ids.is_empty() {
            let s = SubscribeTopics::builder()
                .build()?
                .fake_stream(&self.client);
            let (s, _status) = stream::status_aware(s);
            let s = stream::ordered(
                s,
                self.cursor_store.clone(),
                TopicCursor::default(),
            );
            return Ok(stream::try_extractor(s));
        }
        let topics = group_ids
            .iter()
            .map(|gid| TopicKind::GroupMessagesV1.create(gid))
            .collect::<Vec<_>>();
        let topic_cursor: TopicCursor = self
            .cursor_store
            .latest_for_topics(&mut topics.iter())?
            .into();
        let mut builder = SubscribeTopics::builder();
        for (topic, cursor) in &topic_cursor {
            tracing::debug!("subscribing to messages for topic {} @cursor={}", topic, cursor);
            builder.filter((topic.clone(), cursor.clone()));
        }
        let s = builder
            .build()?
            .stream(&self.client)
            .await?;
        let (s, _status) = stream::status_aware(s);
        let s = stream::ordered(
            s,
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
            let s = SubscribeTopics::builder()
                .build()?
                .fake_stream(&self.client);
            let (s, _status) = stream::status_aware(s);
            let s = stream::ordered(
                s,
                self.cursor_store.clone(),
                TopicCursor::default(),
            );
            return Ok(stream::try_extractor(s));
        }
        let mut builder = SubscribeTopics::builder();
        for (topic, cursor) in topics {
            tracing::debug!(
                "subscribing to messages with provided cursor for topic {} @cursor={}",
                topic,
                cursor
            );
            builder.filter((topic.clone(), cursor.clone()));
        }
        let s = builder
            .build()?
            .stream(&self.client)
            .await?;
        let (s, _status) = stream::status_aware(s);
        let s = stream::ordered(
            s,
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
            let s = SubscribeTopics::builder()
                .build()?
                .fake_stream(&self.client);
            let (s, _status) = stream::status_aware(s);
            return Ok(stream::try_extractor(s));
        }
        let topics = installations
            .iter()
            .map(|ins| TopicKind::WelcomeMessagesV1.create(ins))
            .collect::<Vec<_>>();
        let mut builder = SubscribeTopics::builder();
        for topic in &topics {
            let cursor = self.cursor_store.latest(topic)?;
            tracing::debug!("subscribing to welcome messages for topic {} @cursor={}", topic, cursor);
            builder.filter((topic.clone(), cursor));
        }
        let s = builder
            .build()?
            .stream(&self.client)
            .await?;
        let (s, _status) = stream::status_aware(s);
        Ok(stream::try_extractor(s))
    }
}
