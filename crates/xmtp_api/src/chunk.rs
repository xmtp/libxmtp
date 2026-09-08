//! Request limits, canonical publish units, and complete topic reads.
use crate::{ApiClientWrapper, ApiError, Result, dyn_err};
use futures::{StreamExt, TryStreamExt, stream};
use prost::Message;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    future::Future,
};
use tonic::Code;
use xmtp_common::{RetryableError, retry_async};
use xmtp_configuration::*;
use xmtp_proto::{
    api::grpc_status,
    api_client::XmtpBackendClient,
    backend_v1 as wire,
    types::{CanonicalEnvelope, Cursor, Topic, TopicCursor},
};

pub const MAX_PUBLISH_CHUNKS_IN_FLIGHT: usize = 4;
pub const MAX_READ_CHUNKS_IN_FLIGHT: usize = 4;

#[derive(Clone, Debug)]
struct PublishEnvelope {
    envelope: wire::ClientEnvelope,
    canonical: CanonicalEnvelope,
    topic: Topic,
}

/// One envelope, or a commit and its proposals. A unit is never split.
#[derive(Clone, Debug)]
pub struct PublishUnit {
    envelopes: Vec<PublishEnvelope>,
}
impl PublishUnit {
    pub fn new(envelopes: Vec<wire::ClientEnvelope>) -> Result<Self> {
        if envelopes.is_empty() {
            return Err(ApiError::InvalidRequest("empty publish unit"));
        }
        let envelopes = envelopes
            .into_iter()
            .map(|envelope| {
                let parsed = xmtp_mls_validation::parse_envelope(envelope)?;
                if parsed.canonical.bytes.len() > BACKEND_DEFAULT_MAX_ENVELOPE_BYTES {
                    return Err(ApiError::EnvelopeTooLarge);
                }
                Ok(PublishEnvelope {
                    envelope: parsed.envelope,
                    canonical: parsed.canonical,
                    topic: parsed.topic,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let unit = Self { envelopes };
        let mut measure = PublishMeasure::default();
        measure.add(&unit);
        if !measure.fits() {
            return Err(ApiError::UnitTooLarge);
        }
        Ok(unit)
    }
    pub fn single(envelope: wire::ClientEnvelope) -> Result<Self> {
        Self::new(vec![envelope])
    }
}

pub(crate) fn request(units: &[PublishUnit]) -> wire::PublishRequest {
    wire::PublishRequest {
        envelopes: units
            .iter()
            .flat_map(|unit| unit.envelopes.iter().map(|e| e.envelope.clone()))
            .collect(),
    }
}
/// Measure repeated envelope fields without cloning or encoding the request.
#[derive(Default)]
struct PublishMeasure<'a> {
    bytes: usize,
    topics: HashSet<&'a Topic>,
}
impl<'a> PublishMeasure<'a> {
    fn add(&mut self, unit: &'a PublishUnit) {
        for envelope in &unit.envelopes {
            let len = envelope.canonical.bytes.len();
            self.bytes += 1 + prost::length_delimiter_len(len) + len;
            self.topics.insert(&envelope.topic);
        }
    }

    fn fits(&self) -> bool {
        self.bytes <= BACKEND_DEFAULT_MAX_REQUEST_BYTES
            && self.topics.len() <= BACKEND_DEFAULT_MAX_PUBLISH_TOPICS
    }
}

pub fn chunk_publish(units: &[PublishUnit]) -> Result<Vec<&[PublishUnit]>> {
    let mut chunks = Vec::new();
    let mut start = 0;
    let mut measure = PublishMeasure::default();
    for (end, unit) in units.iter().enumerate() {
        measure.add(unit);
        if !measure.fits() {
            if start == end {
                return Err(ApiError::UnitTooLarge);
            }
            chunks.push(&units[start..end]);
            start = end;
            measure = PublishMeasure::default();
            measure.add(unit);
            if !measure.fits() {
                return Err(ApiError::UnitTooLarge);
            }
        }
    }
    if start < units.len() {
        chunks.push(&units[start..]);
    }
    Ok(chunks)
}

pub(crate) fn size_error(error: &(dyn std::error::Error + 'static)) -> bool {
    let Some(status) = grpc_status(error) else {
        return false;
    };
    match status.code() {
        Code::OutOfRange | Code::ResourceExhausted => true,
        Code::InvalidArgument => {
            tonic_types::pb::Status::decode(status.details()).is_ok_and(|details| {
                details.details.iter().any(|detail| {
                    detail.type_url == "type.googleapis.com/xmtp.backend.v1.PublishError"
                        && wire::PublishError::decode(detail.value.as_slice()).is_ok_and(|error| {
                            error.reason == wire::publish_error::Reason::TooLarge as i32
                        })
                })
            })
        }
        _ => false,
    }
}

/// Size failures bypass backoff while the caller can reduce the request.
#[derive(Debug, thiserror::Error)]
enum CallError<E> {
    #[error(transparent)]
    Resize(E),
    #[error(transparent)]
    Retry(E),
}
impl<E: RetryableError> RetryableError for CallError<E> {
    fn is_retryable(&self) -> bool {
        match self {
            Self::Resize(_) => false,
            Self::Retry(error) => error.is_retryable(),
        }
    }
}

impl<C> ApiClientWrapper<C> {
    pub(crate) async fn retry_call<T, E, F, Fut>(
        &self,
        mut call: F,
        resize: bool,
    ) -> std::result::Result<T, E>
    where
        E: RetryableError + 'static,
        F: FnMut() -> Fut,
        Fut: Future<Output = std::result::Result<T, E>>,
    {
        retry_async!(
            self.retry_strategy,
            (async {
                call().await.map_err(|error| {
                    if resize && size_error(&error) {
                        CallError::Resize(error)
                    } else {
                        CallError::Retry(error)
                    }
                })
            })
        )
        .map_err(|error| match error {
            CallError::Resize(error) | CallError::Retry(error) => error,
        })
    }
}

impl<C: XmtpBackendClient> ApiClientWrapper<C> {
    /// Publish each atomic unit and return metadata in envelope order.
    #[xmtp_common::rpc_span]
    pub async fn publish_units(&self, units: Vec<PublishUnit>) -> Result<Vec<wire::EnvelopeMeta>> {
        let chunks = chunk_publish(&units)?;
        let mut responses: Vec<_> = stream::iter(
            chunks
                .into_iter()
                .enumerate()
                .map(|(index, chunk)| async move {
                    Ok::<_, ApiError>((index, self.publish_chunk(chunk).await?))
                }),
        )
        .buffer_unordered(MAX_PUBLISH_CHUNKS_IN_FLIGHT)
        .try_collect()
        .await?;
        responses.sort_by_key(|(index, _)| *index);
        Ok(responses.into_iter().flat_map(|(_, metas)| metas).collect())
    }

    async fn publish_chunk(&self, units: &[PublishUnit]) -> Result<Vec<wire::EnvelopeMeta>> {
        let mut pending = VecDeque::from([units]);
        let mut metas = Vec::new();
        while let Some(units) = pending.pop_front() {
            let request = request(units);
            match self
                .retry_call(|| self.api_client.publish(request.clone()), units.len() > 1)
                .await
            {
                Ok(response) => {
                    let expected: Vec<_> = units.iter().flat_map(|unit| &unit.envelopes).collect();
                    if response.envelope_metas.len() != expected.len() {
                        return Err(ApiError::InvalidResponse("publish metadata count"));
                    }
                    for (meta, envelope) in response.envelope_metas.iter().zip(expected) {
                        let hash = xmtp_api_d14n::envelope::message_hash(meta)?;
                        if hash != envelope.canonical.hash {
                            return Err(ApiError::HashMismatch);
                        }
                        let (topic, _, _) =
                            xmtp_api_d14n::envelope::metadata(meta, envelope.topic.kind())?;
                        if topic != envelope.topic {
                            return Err(ApiError::InvalidResponse("publish topic"));
                        }
                    }
                    metas.extend(response.envelope_metas);
                }
                Err(error) if size_error(&error) && units.len() > 1 => {
                    let (left, right) = units.split_at(units.len() / 2);
                    pending.push_front(right);
                    pending.push_front(left);
                }
                Err(error) => return Err(dyn_err(error)),
            }
        }
        Ok(metas)
    }

    /// Read every page. Each topic advances only to its own returned cursor.
    pub async fn query_all(
        &self,
        cursors: TopicCursor,
        limit: u32,
    ) -> Result<Vec<wire::ServerEnvelope>> {
        if limit == 0 || limit as usize > BACKEND_DEFAULT_MAX_QUERY_LIMIT {
            return Err(ApiError::InvalidRequest("query limit"));
        }
        let topics: Vec<_> = cursors.into_iter().collect();
        let results: Vec<Vec<_>> = stream::iter(
            topics
                .chunks(BACKEND_DEFAULT_MAX_QUERY_TOPICS)
                .map(|chunk| self.query_chunk(chunk.to_vec(), limit)),
        )
        .buffer_unordered(MAX_READ_CHUNKS_IN_FLIGHT)
        .try_collect()
        .await?;
        Ok(results.into_iter().flatten().collect())
    }

    async fn query_chunk(
        &self,
        topics: Vec<(Topic, Cursor)>,
        limit: u32,
    ) -> Result<Vec<wire::ServerEnvelope>> {
        let mut pending = VecDeque::from([(topics, limit)]);
        let mut output = Vec::new();
        while let Some((mut topics, mut limit)) = pending.pop_front() {
            loop {
                let request = wire::QueryRequest {
                    queries: topics
                        .iter()
                        .map(|(topic, cursor)| wire::TopicQuery {
                            topic: Some(wire::Topic {
                                topic: topic.cloned_vec(),
                            }),
                            cursor: Some((*cursor).into()),
                        })
                        .collect(),
                    limit,
                };
                match self
                    .retry_call(
                        || self.api_client.query(request.clone()),
                        limit > 1 || topics.len() > 1,
                    )
                    .await
                {
                    Ok(response) => {
                        let has_more = response
                            .continuation
                            .ok_or(ApiError::InvalidResponse("query continuation"))?
                            .has_more;
                        let mut cursors: HashMap<_, _> = topics
                            .iter_mut()
                            .map(|(topic, cursor)| (topic.clone(), cursor))
                            .collect();
                        let mut advanced = false;
                        for envelope in &response.envelopes {
                            let meta = envelope
                                .meta
                                .as_ref()
                                .ok_or(ApiError::InvalidResponse("query metadata"))?;
                            let topic = Topic::parse(
                                &meta
                                    .topic
                                    .as_ref()
                                    .ok_or(ApiError::InvalidResponse("query topic"))?
                                    .topic,
                            )?;
                            let cursor = cursors
                                .get_mut(&topic)
                                .ok_or(ApiError::InvalidResponse("unrequested query topic"))?;
                            let sequence = meta
                                .cursor
                                .as_ref()
                                .ok_or(ApiError::InvalidResponse("query cursor"))?
                                .sequence_id;
                            if sequence <= cursor.0 || sequence > i64::MAX as u64 {
                                return Err(ApiError::InvalidResponse(
                                    "query cursor did not advance",
                                ));
                            }
                            **cursor = Cursor(sequence);
                            advanced = true;
                        }
                        output.extend(response.envelopes);
                        if !has_more {
                            break;
                        }
                        if !advanced {
                            return Err(ApiError::InvalidResponse(
                                "query has more without progress",
                            ));
                        }
                    }
                    Err(error) if size_error(&error) && limit > 1 => {
                        limit = (limit / 2).max(1);
                    }
                    Err(error) if size_error(&error) && topics.len() > 1 => {
                        let right = topics.split_off(topics.len() / 2);
                        pending.push_front((right, limit));
                    }
                    Err(error) => return Err(dyn_err(error)),
                }
            }
        }
        Ok(output)
    }

    pub(crate) async fn newest(
        &self,
        topics: Vec<Topic>,
        full: bool,
    ) -> Result<Vec<wire::query_newest_response::Result>> {
        let topics: Vec<_> = topics
            .into_iter()
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        let cap = if full {
            BACKEND_DEFAULT_MAX_NEWEST_FULL_TOPICS
        } else {
            BACKEND_DEFAULT_MAX_NEWEST_METADATA_TOPICS
        };
        let results: Vec<Vec<_>> = stream::iter(
            topics
                .chunks(cap)
                .map(|chunk| self.newest_chunk(chunk, full)),
        )
        .buffer_unordered(MAX_READ_CHUNKS_IN_FLIGHT)
        .try_collect()
        .await?;
        Ok(results.into_iter().flatten().collect())
    }

    async fn newest_chunk(
        &self,
        topics: &[Topic],
        full: bool,
    ) -> Result<Vec<wire::query_newest_response::Result>> {
        let mut pending = VecDeque::from([topics]);
        let mut output = Vec::new();
        while let Some(topics) = pending.pop_front() {
            let request = wire::QueryNewestRequest {
                topics: topics
                    .iter()
                    .map(|topic| wire::Topic {
                        topic: topic.cloned_vec(),
                    })
                    .collect(),
                include_full_envelope: full,
            };
            match self
                .retry_call(
                    || self.api_client.query_newest(request.clone()),
                    topics.len() > 1,
                )
                .await
            {
                Ok(response) => {
                    let mut seen = HashSet::new();
                    for result in &response.results {
                        let topic = Topic::parse(
                            &result
                                .topic
                                .as_ref()
                                .ok_or(ApiError::InvalidResponse("newest topic"))?
                                .topic,
                        )?;
                        if !topics.contains(&topic) || !seen.insert(topic.clone()) {
                            return Err(ApiError::InvalidResponse(
                                "unrequested or duplicate newest topic",
                            ));
                        }
                        let meta = result
                            .meta
                            .as_ref()
                            .ok_or(ApiError::InvalidResponse("newest metadata"))?;
                        let (meta_topic, _, _) =
                            xmtp_api_d14n::envelope::metadata(meta, topic.kind())?;
                        if meta_topic != topic || (full && result.envelope.is_none()) {
                            return Err(ApiError::InvalidResponse("newest envelope"));
                        }
                    }
                    output.extend(response.results);
                }
                Err(error) if size_error(&error) && topics.len() > 1 => {
                    let (left, right) = topics.split_at(topics.len() / 2);
                    pending.push_front(right);
                    pending.push_front(left);
                }
                Err(error) => return Err(dyn_err(error)),
            }
        }
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use xmtp_mls_validation::test_utils::{
        GroupMessageKind, group_message_envelope, inline_welcome_envelope,
    };

    #[xmtp_common::test(unwrap_try = true)]
    fn running_publish_measure_matches_encoded_mixed_request() {
        let units = [
            PublishUnit::single(inline_welcome_envelope([1; 32]))?,
            PublishUnit::new(vec![
                group_message_envelope([2; 16], GroupMessageKind::Proposal, [0; 127]),
                group_message_envelope([2; 16], GroupMessageKind::Commit, [0; 16384]),
            ])?,
            PublishUnit::single(inline_welcome_envelope([1; 32]))?,
            PublishUnit::single(inline_welcome_envelope([3; 32]))?,
        ];
        let mut measure = PublishMeasure::default();
        for (index, unit) in units.iter().enumerate() {
            measure.add(unit);
            assert_eq!(measure.bytes, request(&units[..=index]).encoded_len());
        }
        assert_eq!(measure.topics.len(), 3);
    }
}
