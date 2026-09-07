use crate::{
    Backend,
    api::{self, client_envelope::Payload, publish_error::Reason},
    db::PendingEnvelope,
    error::{publish_invalid, validation_status},
    validation::{projection, scw_count},
};
use prost::Message;
use std::collections::{HashMap, HashSet};
use tonic::{Request, Response, Status};
use xmtp_common::time::{Duration, timeout};
use xmtp_mls_validation::{parse_envelope, validate_envelope};
use xmtp_proto::types::TopicKind;

struct PublishBatch {
    pending: Vec<PendingEnvelope>,
    positions: Vec<usize>,
    parse_error: Option<(usize, Status)>,
}

#[tonic::async_trait]
impl api::publish_service_server::PublishService for Backend {
    #[xmtp_common::rpc_span]
    async fn publish(
        &self,
        request: Request<api::PublishRequest>,
    ) -> Result<Response<api::PublishResponse>, Status> {
        let request = request.into_inner();
        let PublishBatch {
            mut pending,
            positions,
            parse_error,
        } = self.parse_publish(request)?;
        self.validate_publish(&mut pending).await?;
        let duration = Duration::from_millis(self.config.publishing.max_publish_duration_ms);
        let metas = timeout(
            duration,
            self.store.commit_publish(
                &mut pending,
                parse_error,
                self.config.publishing.max_publish_duration_ms,
            ),
        )
        .await
        .map_err(|_| Status::deadline_exceeded("publish timed out"))??;
        let envelope_metas = positions
            .into_iter()
            .map(|position| metas[position].clone().wire())
            .collect();
        Ok(Response::new(api::PublishResponse { envelope_metas }))
    }
}

impl Backend {
    fn parse_publish(&self, request: api::PublishRequest) -> Result<PublishBatch, Status> {
        let limits = &self.config.limits;
        if request.encoded_len() > limits.max_request_bytes {
            return Err(publish_invalid(
                None,
                Reason::TooLarge,
                "publish request exceeds byte limit",
            ));
        }
        let mut unique = HashMap::new();
        let mut topics = HashSet::new();
        let mut identity_topics = HashSet::new();
        let mut pending = Vec::new();
        let mut positions = Vec::with_capacity(request.envelopes.len());
        let mut parse_error = None;
        for (index, envelope) in request.envelopes.into_iter().enumerate() {
            if envelope.encoded_len() > limits.max_envelope_bytes {
                parse_error.get_or_insert((
                    index,
                    publish_invalid(Some(index), Reason::TooLarge, "envelope exceeds byte limit"),
                ));
                continue;
            }
            let parsed = match parse_envelope(envelope) {
                Ok(parsed) => parsed,
                Err(error) => {
                    parse_error.get_or_insert((index, validation_status(index, error)));
                    continue;
                }
            };
            let key = (parsed.topic.clone(), parsed.canonical.hash);
            if let Some(&position) = unique.get(&key) {
                positions.push(position);
                continue;
            }
            if parsed.topic.kind() == TopicKind::IdentityUpdatesV1
                && !identity_topics.insert(parsed.topic.clone())
            {
                parse_error.get_or_insert((
                    index,
                    publish_invalid(
                        Some(index),
                        Reason::InvalidIdentityUpdate,
                        "distinct updates address one inbox",
                    ),
                ));
                continue;
            }
            topics.insert(parsed.topic.clone());
            if topics.len() > limits.max_publish_topics {
                return Err(publish_invalid(
                    None,
                    Reason::TooLarge,
                    "publish exceeds topic limit",
                ));
            }
            unique.insert(key, pending.len());
            positions.push(pending.len());
            let retention = match parsed.topic.kind() {
                TopicKind::GroupMessagesV1 if !parsed.is_commit_or_proposal => {
                    Some(self.config.retention.group_message_seconds)
                }
                TopicKind::WelcomeMessagesV1 => Some(self.config.retention.welcome_seconds),
                TopicKind::KeyPackagesV1 => Some(self.config.retention.key_package_seconds),
                _ => None,
            };
            let retention_ns = retention
                .map(|seconds| {
                    i64::try_from(seconds)
                        .ok()
                        .and_then(|seconds| seconds.checked_mul(xmtp_common::NS_IN_SEC))
                        .ok_or_else(|| Status::internal("invalid retention duration"))
                })
                .transpose()?;
            pending.push(PendingEnvelope {
                parsed,
                index,
                duplicate: None,
                validation: Ok(None),
                head: 0,
                retention_ns,
            });
        }
        Ok(PublishBatch {
            pending,
            positions,
            parse_error,
        })
    }

    async fn validate_publish(&self, pending: &mut [PendingEnvelope]) -> Result<(), Status> {
        self.store.find_duplicates(pending).await?;
        for item in pending {
            if item.duplicate.is_some() {
                continue;
            }
            let history =
                if let Some(Payload::IdentityUpdate(update)) = &item.parsed.envelope.payload {
                    let history = self.store.history(&item.parsed.topic).await?;
                    item.head = history.head;
                    if scw_count(update) > self.config.limits.max_scw_signatures {
                        item.validation = Err(publish_invalid(
                            Some(item.index),
                            Reason::TooLarge,
                            "identity update exceeds signature limit",
                        ));
                        continue;
                    }
                    history.updates
                } else {
                    Vec::new()
                };
            item.validation = validate_envelope(&item.parsed, &history, &self.verifier)
                .await
                .map(|result| result.as_ref().map(projection))
                .map_err(|error| validation_status(item.index, error));
            if item.validation.is_ok() && history.len() >= self.config.limits.max_identity_entries {
                item.validation = Err(publish_invalid(
                    Some(item.index),
                    Reason::InvalidIdentityUpdate,
                    "identity history limit reached",
                ));
            }
        }
        Ok(())
    }
}
