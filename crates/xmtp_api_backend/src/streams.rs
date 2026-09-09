use crate::{
    BackendClient,
    backend::SubscribeStatic,
    envelope::{decode_group_message, decode_welcome_message},
    queries::stream::try_extractor,
};
use futures::{StreamExt, stream};
use xmtp_common::{
    BoxDynStream,
    time::{Duration, timeout},
};
use xmtp_configuration::{
    BACKEND_DEFAULT_KEEPALIVE_INTERVAL_MS, BACKEND_DEFAULT_MAX_NEWEST_METADATA_TOPICS,
    BACKEND_DEFAULT_MAX_STATIC_TOPICS,
};
use xmtp_proto::{
    api::{ApiClientError, Client, QueryStreamExt},
    api_client::{XmtpBackendClient, XmtpMlsStreams},
    backend_v1 as wire,
    types::{Cursor, GroupId, GroupMessage, InstallationId, Topic, TopicCursor, WelcomeMessage},
};

const SILENT_INTERVALS: u32 = 3;

impl<C: Client> BackendClient<C> {
    async fn newest_cursors(
        &self,
        topics: impl IntoIterator<Item = Topic>,
    ) -> Result<TopicCursor, ApiClientError> {
        let mut cursors: TopicCursor = topics.into_iter().map(|topic| (topic, Cursor(0))).collect();
        let topics: Vec<_> = cursors.keys().cloned().collect();
        for chunk in topics.chunks(BACKEND_DEFAULT_MAX_NEWEST_METADATA_TOPICS) {
            let response = self
                .query_newest(wire::QueryNewestRequest {
                    topics: chunk
                        .iter()
                        .map(|t| wire::Topic {
                            topic: t.cloned_vec(),
                        })
                        .collect(),
                    include_full_envelope: false,
                })
                .await?;
            for result in response.results {
                let topic =
                    Topic::parse(&result.topic.ok_or_else(|| malformed("newest topic"))?.topic)?;
                let cursor = result
                    .meta
                    .and_then(|m| m.cursor)
                    .ok_or_else(|| malformed("newest cursor"))?;
                *cursors
                    .get_mut(&topic)
                    .ok_or_else(|| malformed("unrequested newest topic"))? = cursor.into();
            }
        }
        Ok(cursors)
    }

    async fn static_envelopes(
        &self,
        cursors: &TopicCursor,
    ) -> Result<
        BoxDynStream<'static, Result<Vec<wire::ServerEnvelope>, ApiClientError>>,
        ApiClientError,
    > {
        if cursors.is_empty() {
            return Ok(Box::pin(stream::pending()));
        }
        let queries: Vec<_> = cursors
            .iter()
            .map(|(topic, cursor)| wire::TopicQuery {
                topic: Some(wire::Topic {
                    topic: topic.cloned_vec(),
                }),
                cursor: Some((*cursor).into()),
            })
            .collect();
        let mut streams = Vec::new();
        for chunk in queries.chunks(BACKEND_DEFAULT_MAX_STATIC_TOPICS) {
            let stream = SubscribeStatic(wire::SubscribeStaticRequest {
                topics: chunk.to_vec(),
            })
            .subscribe(&self.client)
            .await?;
            let stream = stream::unfold(
                (
                    stream,
                    Duration::from_millis(BACKEND_DEFAULT_KEEPALIVE_INTERVAL_MS),
                    false,
                ),
                |(mut stream, mut interval, ended)| async move {
                    if ended {
                        return None;
                    }
                    loop {
                        match timeout(interval * SILENT_INTERVALS, stream.next()).await {
                            Err(error) => {
                                return Some((
                                    Err(ApiClientError::Expired(error)),
                                    (stream, interval, true),
                                ));
                            }
                            Ok(None) => return None,
                            Ok(Some(Err(error))) => {
                                return Some((Err(error), (stream, interval, true)));
                            }
                            Ok(Some(Ok(frame))) => match frame.response {
                                Some(wire::subscribe_static_response::Response::Started(
                                    started,
                                )) => {
                                    if started.keepalive_interval_ms != 0 {
                                        interval = Duration::from_millis(
                                            started.keepalive_interval_ms.into(),
                                        );
                                    }
                                }
                                Some(wire::subscribe_static_response::Response::Keepalive(_)) => {}
                                Some(wire::subscribe_static_response::Response::Messages(
                                    messages,
                                )) => {
                                    return Some((
                                        Ok(messages.envelopes),
                                        (stream, interval, false),
                                    ));
                                }
                                None => {
                                    return Some((
                                        Err(malformed("static response")),
                                        (stream, interval, true),
                                    ));
                                }
                            },
                        }
                    }
                },
            );
            streams.push(Box::pin(stream) as BoxDynStream<'static, _>);
        }
        // A wire error invalidates the complete subscription. Reopen all topics from durable cursors.
        Ok(Box::pin(stream::unfold(
            Some(stream::select_all(streams)),
            |streams| async move {
                let mut streams = streams?;
                let item: Result<Vec<wire::ServerEnvelope>, ApiClientError> =
                    streams.next().await?;
                let remaining = if item.is_err() { None } else { Some(streams) };
                Some((item, remaining))
            },
        )))
    }
}
fn malformed(field: &str) -> ApiClientError {
    ApiClientError::OtherUnretryable(format!("missing or invalid {field}").into())
}

#[xmtp_common::async_trait]
impl<C: Client> XmtpMlsStreams for BackendClient<C> {
    type Error = ApiClientError;
    type GroupMessageStream = BoxDynStream<'static, Result<GroupMessage, ApiClientError>>;
    type WelcomeMessageStream = BoxDynStream<'static, Result<WelcomeMessage, ApiClientError>>;
    async fn subscribe_group_messages(
        &self,
        groups: &[&GroupId],
    ) -> Result<Self::GroupMessageStream, Self::Error> {
        let cursors = self
            .newest_cursors(
                groups
                    .iter()
                    .map(|id| Topic::new_group_message(id.as_ref())),
            )
            .await?;
        self.subscribe_group_messages_with_cursors(&cursors).await
    }
    async fn subscribe_group_messages_with_cursors(
        &self,
        cursors: &TopicCursor,
    ) -> Result<Self::GroupMessageStream, Self::Error> {
        Ok(Box::pin(try_extractor(
            self.static_envelopes(cursors).await?,
            |envelope| decode_group_message(envelope).map_err(Into::into),
        )))
    }
    async fn subscribe_welcome_messages(
        &self,
        installations: &[&InstallationId],
    ) -> Result<Self::WelcomeMessageStream, Self::Error> {
        let cursors = self
            .newest_cursors(
                installations
                    .iter()
                    .map(|id| Topic::new_welcome_message(**id)),
            )
            .await?;
        self.subscribe_welcome_messages_with_cursors(&cursors).await
    }
    async fn subscribe_welcome_messages_with_cursors(
        &self,
        cursors: &TopicCursor,
    ) -> Result<Self::WelcomeMessageStream, Self::Error> {
        Ok(Box::pin(try_extractor(
            self.static_envelopes(cursors).await?,
            |envelope| decode_welcome_message(envelope).map_err(Into::into),
        )))
    }
}

#[cfg(test)]
mod tests;
