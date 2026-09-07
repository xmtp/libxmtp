use crate::{Backend, api, db::TopicCursor};
use std::collections::HashMap;
use tonic::{Request, Response, Status};
use xmtp_proto::types::{Topic, TopicKind};

pub(crate) fn topic(value: &api::Topic) -> Result<Topic, Status> {
    Topic::parse(&value.topic).map_err(|_| Status::invalid_argument("invalid topic"))
}

pub(crate) fn cursor(value: u64) -> Result<i64, Status> {
    i64::try_from(value).map_err(|_| Status::invalid_argument("cursor exceeds signed 64-bit range"))
}

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

#[cfg(test)]
mod tests {
    use super::*;

    fn request(topic: Topic, cursor: Option<u64>) -> api::TopicQuery {
        api::TopicQuery {
            topic: Some(api::Topic {
                topic: topic.to_vec(),
            }),
            cursor: cursor.map(|sequence_id| api::Cursor { sequence_id }),
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn duplicate_query_inputs_become_one_database_cursor_at_the_lowest_position() {
        let first = TopicKind::WelcomeMessagesV1.create([1; 32]);
        let second = TopicKind::WelcomeMessagesV1.create([2; 32]);
        let queries = coalesce_queries(
            vec![
                request(first.clone(), Some(20)),
                request(second.clone(), Some(30)),
                request(first.clone(), Some(10)),
                request(second.clone(), None),
            ],
            4,
        )?;
        assert_eq!(queries.len(), 2);
        let actual: HashMap<_, _> = queries
            .into_iter()
            .map(|query| (query.topic, query.cursor))
            .collect();
        assert_eq!(
            actual,
            HashMap::from([(first.to_vec(), 10), (second.to_vec(), 0)])
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn duplicate_inputs_do_not_bypass_cursor_validation_or_original_count_limits() {
        let topic = TopicKind::WelcomeMessagesV1.create([1; 32]);
        let invalid_cursor = coalesce_queries(
            vec![
                request(topic.clone(), None),
                request(topic.clone(), Some(u64::MAX)),
            ],
            2,
        );
        assert!(
            matches!(invalid_cursor, Err(error) if error.code() == tonic::Code::InvalidArgument)
        );
        let excess = coalesce_queries(vec![request(topic.clone(), None), request(topic, None)], 1);
        assert!(matches!(excess, Err(error) if error.code() == tonic::Code::InvalidArgument));
    }
}
