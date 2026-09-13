use crate::{
    BackendClient,
    backend::SubscribeStatic,
    envelope::{
        decode_group_message, decode_welcome_message, ordered_batches, registration_targets,
    },
    queries::stream::try_extractor,
};
use futures::{StreamExt, stream};
use std::collections::VecDeque;
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
    types::{
        Cursor, GroupId, GroupMessage, IncomingBatchLimits, IncomingEvent, IncomingSubscription,
        InstallationId, Topic, TopicCursor, WelcomeMessage,
    },
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

    /// Split static topic interest while preserving each registration's targets.
    /// Any feed error ends all chunks; the caller reopens from durable receipt.
    async fn static_events(
        &self,
        cursors: &TopicCursor,
        limits: IncomingBatchLimits,
    ) -> Result<BoxDynStream<'static, Result<IncomingEvent, ApiClientError>>, ApiClientError> {
        if cursors.is_empty() {
            return Ok(Box::pin(stream::pending()));
        }
        let topics: Vec<_> = cursors
            .iter()
            .map(|(topic, cursor)| (topic.clone(), *cursor))
            .collect();
        let mut streams = Vec::new();
        for chunk in topics.chunks(BACKEND_DEFAULT_MAX_STATIC_TOPICS) {
            let stream = SubscribeStatic(wire::SubscribeStaticRequest {
                topics: chunk
                    .iter()
                    .map(|(topic, cursor)| wire::TopicQuery {
                        topic: Some(wire::Topic {
                            topic: topic.cloned_vec(),
                        }),
                        cursor: Some((*cursor).into()),
                    })
                    .collect(),
            })
            .subscribe(&self.client)
            .await?;
            streams.push(normalize_static_stream(
                Box::pin(stream),
                chunk.iter().cloned().collect(),
                limits,
            ));
        }
        // A wire error invalidates the complete subscription. Reopen all topics from durable cursors.
        Ok(Box::pin(stream::unfold(
            Some(stream::select_all(streams)),
            |streams| async move {
                let mut streams = streams?;
                let item = streams.next().await?;
                let ended = item.is_err() || matches!(item, Ok(IncomingEvent::Disconnected));
                let remaining = if ended { None } else { Some(streams) };
                Some((item, remaining))
            },
        )))
    }

    async fn static_envelopes(
        &self,
        cursors: &TopicCursor,
    ) -> Result<
        BoxDynStream<'static, Result<Vec<wire::ServerEnvelope>, ApiClientError>>,
        ApiClientError,
    > {
        let events = self
            .static_events(
                cursors,
                IncomingBatchLimits {
                    max_rows: xmtp_configuration::BACKEND_DEFAULT_MAX_QUERY_LIMIT,
                    max_bytes: xmtp_configuration::BACKEND_DEFAULT_MAX_REQUEST_BYTES,
                },
            )
            .await?;
        Ok(Box::pin(events.filter_map(|event| async move {
            match event {
                Ok(IncomingEvent::OrderedBatch(batch)) => Some(Ok(batch.envelopes)),
                Ok(IncomingEvent::Registered { .. }) => None,
                Ok(IncomingEvent::Disconnected) => None,
                Err(error) => Some(Err(error)),
            }
        })))
    }
}

/// Require `Started` before data and retain its fixed catch-up targets.
/// Validate each complete frame before emitting batches; receipt stays external.
fn normalize_static_stream(
    stream: BoxDynStream<'static, Result<wire::SubscribeStaticResponse, ApiClientError>>,
    starts: TopicCursor,
    limits: IncomingBatchLimits,
) -> BoxDynStream<'static, Result<IncomingEvent, ApiClientError>> {
    let cursors = starts.clone();
    Box::pin(stream::unfold(
        (
            stream,
            Duration::from_millis(BACKEND_DEFAULT_KEEPALIVE_INTERVAL_MS),
            starts,
            cursors,
            false,
            VecDeque::new(),
            false,
        ),
        move |(
            mut stream,
            mut interval,
            starts,
            mut cursors,
            mut registered,
            mut pending,
            mut ended,
        )| async move {
            if ended {
                return None;
            }
            let item = loop {
                if let Some(event) = pending.pop_front() {
                    break Ok(event);
                }
                let frame = match timeout(interval * SILENT_INTERVALS, stream.next()).await {
                    Err(error) => break Err(ApiClientError::Expired(error)),
                    Ok(None) => break Ok(IncomingEvent::Disconnected),
                    Ok(Some(Err(error))) => break Err(error),
                    Ok(Some(Ok(frame))) => frame,
                };
                match frame.response {
                    Some(wire::subscribe_static_response::Response::Started(started))
                        if !registered =>
                    {
                        let targets = match registration_targets(&starts, started.targets) {
                            Ok(targets) => targets,
                            Err(error) => break Err(error.into()),
                        };
                        registered = true;
                        if started.keepalive_interval_ms != 0 {
                            interval = Duration::from_millis(started.keepalive_interval_ms.into());
                        }
                        break Ok(IncomingEvent::Registered {
                            starts: starts.clone(),
                            targets,
                        });
                    }
                    Some(wire::subscribe_static_response::Response::Keepalive(_)) if registered => {
                    }
                    Some(wire::subscribe_static_response::Response::Messages(messages))
                        if registered =>
                    {
                        match ordered_batches(&mut cursors, messages.envelopes, limits) {
                            Ok(batches) => {
                                pending.extend(batches.into_iter().map(IncomingEvent::OrderedBatch))
                            }
                            Err(error) => break Err(error.into()),
                        }
                    }
                    _ => break Err(malformed("static response order")),
                }
            };
            ended = item.is_err() || matches!(item, Ok(IncomingEvent::Disconnected));
            Some((
                item,
                (
                    stream, interval, starts, cursors, registered, pending, ended,
                ),
            ))
        },
    ))
}
fn malformed(field: &str) -> ApiClientError {
    ApiClientError::OtherUnretryable(format!("missing or invalid {field}").into())
}

#[xmtp_common::async_trait]
impl<C: Client> XmtpMlsStreams for BackendClient<C> {
    type Error = ApiClientError;
    type GroupMessageStream = BoxDynStream<'static, Result<GroupMessage, ApiClientError>>;
    type WelcomeMessageStream = BoxDynStream<'static, Result<WelcomeMessage, ApiClientError>>;
    async fn subscribe_envelopes_with_cursors(
        &self,
        cursors: &TopicCursor,
        limits: IncomingBatchLimits,
    ) -> Result<IncomingSubscription<Self::Error>, Self::Error> {
        Ok(IncomingSubscription::new(
            self.static_events(cursors, limits).await?,
            |_| {},
        ))
    }
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
