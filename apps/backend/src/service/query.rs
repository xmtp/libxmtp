use crate::{Backend, api};
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
        count(request.queries.len(), limits.max_query_topics)?;
        let mut topics = Vec::with_capacity(request.queries.len());
        let mut cursors = Vec::with_capacity(request.queries.len());
        for query in request.queries {
            let value = query
                .topic
                .ok_or_else(|| Status::invalid_argument("topic is absent"))?;
            let parsed = topic(&value)?;
            if parsed.kind() == TopicKind::KeyPackagesV1 {
                return Err(Status::invalid_argument("key packages require QueryNewest"));
            }
            topics.push(value.topic);
            cursors.push(cursor(query.cursor.map_or(0, |cursor| cursor.sequence_id))?);
        }
        let limit = if request.limit == 0 {
            limits.default_query_limit
        } else {
            (request.limit as usize).min(limits.max_query_limit)
        };
        Ok(Response::new(
            self.store.query(topics, cursors, limit as i64).await?,
        ))
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
        let topics = request
            .topics
            .into_iter()
            .map(|topic| topic.topic)
            .collect();
        Ok(Response::new(
            self.store
                .newest(topics, request.include_full_envelope)
                .await?,
        ))
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
            .map(Response::new)
            .ok_or_else(|| Status::not_found("envelope not found"))
    }
}
