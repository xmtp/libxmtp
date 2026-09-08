use crate::{Backend, api, db::TopicCursor};
use std::collections::HashMap;
use tonic::{Request, Response, Status};
use xmtp_proto::types::{Topic, TopicKind};

#[cfg(test)]
mod tests;

/// Parse a wire topic and reject unknown kinds or invalid identifier lengths.
pub(crate) fn topic(value: &api::Topic) -> Result<Topic, Status> {
    Topic::parse(&value.topic).map_err(|_| Status::invalid_argument("invalid topic"))
}

/// Convert a wire cursor to the signed database sequence type.
///
/// Sequence IDs are non-negative on the wire. Values above the signed
/// database range are rejected instead of wrapping.
pub(crate) fn cursor(value: u64) -> Result<i64, Status> {
    i64::try_from(value).map_err(|_| Status::invalid_argument("cursor exceeds signed 64-bit range"))
}

/// Enforce a collection limit before any normalization or coalescing.
pub(crate) fn count(actual: usize, max: usize) -> Result<(), Status> {
    if actual > max {
        Err(Status::invalid_argument("request exceeds item limit"))
    } else {
        Ok(())
    }
}

#[tonic::async_trait]
impl api::query_service_server::QueryService for Backend {
    #[xmtp_common::rpc_span]
    /// Return a total-limit page across the requested topic cursors.
    ///
    /// Duplicate topic inputs are coalesced at their lowest cursor before the
    /// primary read. The response converts stored rows only after the database
    /// has computed `has_more` from the same snapshot.
    async fn query(
        &self,
        request: Request<api::QueryRequest>,
    ) -> Result<Response<api::QueryResponse>, Status> {
        let request = request.into_inner();
        let limits = &self.config.limits;
        let queries = coalesce_queries(request.queries, limits.max_query_topics)?;
        let limit = if request.limit == 0 {
            limits.default_query_limit
        } else {
            (request.limit as usize).min(limits.max_query_limit)
        };
        let page = self.store.query(&queries, limit as i64).await?;
        Ok(Response::new(api::QueryResponse {
            envelopes: page
                .envelopes
                .into_iter()
                .map(api::ServerEnvelope::try_from)
                .collect::<Result<_, _>>()?,
            continuation: Some(api::Continuation {
                has_more: page.has_more,
            }),
        }))
    }

    #[xmtp_common::rpc_span]
    /// Return the newest visible row for each requested topic.
    ///
    /// Metadata-only requests avoid payload loading. Full requests use the read
    /// pool and omit topics with no visible watermark.
    async fn query_newest(
        &self,
        request: Request<api::QueryNewestRequest>,
    ) -> Result<Response<api::QueryNewestResponse>, Status> {
        let request = request.into_inner();
        let limits = &self.config.limits;
        let max = if request.include_full_envelope {
            limits.max_newest_full_topics
        } else {
            limits.max_newest_metadata_topics
        };
        count(request.topics.len(), max)?;
        for value in &request.topics {
            topic(value)?;
        }
        let topics: Vec<_> = request
            .topics
            .into_iter()
            .map(|topic| topic.topic)
            .collect();
        let results = if request.include_full_envelope {
            self.store
                .newest_envelopes(&topics)
                .await?
                .into_iter()
                .map(|row| {
                    let wire = api::ServerEnvelope::try_from(row)?;
                    Ok(api::query_newest_response::Result {
                        topic: wire.meta.as_ref().and_then(|meta| meta.topic.clone()),
                        meta: wire.meta,
                        envelope: wire.envelope,
                    })
                })
                .collect::<Result<Vec<_>, Status>>()?
        } else {
            self.store
                .newest_metadata(&topics)
                .await?
                .into_iter()
                .map(|row| {
                    let meta: api::EnvelopeMeta = row.into();
                    api::query_newest_response::Result {
                        topic: meta.topic.clone(),
                        meta: Some(meta),
                        envelope: None,
                    }
                })
                .collect()
        };
        Ok(Response::new(api::QueryNewestResponse { results }))
    }

    #[xmtp_common::rpc_span]
    /// Fetch one envelope by its positive global sequence ID.
    ///
    /// A missing row is reported as `NOT_FOUND` without distinguishing replica
    /// lag from any other absence.
    async fn get(
        &self,
        request: Request<api::GetRequest>,
    ) -> Result<Response<api::ServerEnvelope>, Status> {
        let id = cursor(request.into_inner().sequence_id)?;
        if id == 0 {
            return Err(Status::invalid_argument("sequence id must be positive"));
        }
        self.store
            .get(id)
            .await?
            .ok_or_else(|| Status::not_found("envelope not found"))?
            .try_into()
            .map(Response::new)
    }
}

/// Validate and coalesce topic cursors for one query request.
///
/// The original input count is checked before coalescing. Every topic and
/// cursor is still validated, and repeated topics use the lowest cursor so no
/// requested history is skipped.
fn coalesce_queries(queries: Vec<api::TopicQuery>, max: usize) -> Result<Vec<TopicCursor>, Status> {
    count(queries.len(), max)?;
    let mut unique = HashMap::new();
    for query in queries {
        let value = query
            .topic
            .ok_or_else(|| Status::invalid_argument("topic is absent"))?;
        let parsed = topic(&value)?;
        if parsed.kind() == TopicKind::KeyPackagesV1 {
            return Err(Status::invalid_argument("key packages require QueryNewest"));
        }
        let cursor = cursor(query.cursor.map_or(0, |cursor| cursor.sequence_id))?;
        unique
            .entry(value.topic)
            .and_modify(|old: &mut i64| *old = (*old).min(cursor))
            .or_insert(cursor);
    }
    Ok(unique
        .into_iter()
        .map(|(topic, cursor)| TopicCursor { topic, cursor })
        .collect())
}
