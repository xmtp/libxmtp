use super::error::publish_invalid;
use crate::{
    Backend,
    api::{self, client_envelope::Payload, publish_error::Reason},
    db::{IdentityAdmission, PendingEnvelope},
    error::AdmissionError,
    validation::{projection, scw_count},
};
use prost::Message;
use std::collections::{HashMap, HashSet};
use tonic::{Request, Response, Status};
use xmtp_common::time::{Duration, timeout};
use xmtp_mls_validation::{ParsedEnvelope, parse_envelope, validate_envelope};
use xmtp_proto::types::TopicKind;

#[cfg(test)]
mod tests;

struct PublishBatch {
    pending: Vec<PendingEnvelope>,
    parsed: Vec<ParsedEnvelope>,
    positions: Vec<usize>,
    parse_error: Option<(usize, AdmissionError)>,
}

#[tonic::async_trait]
impl api::publish_service_server::PublishService for Backend {
    #[xmtp_common::rpc_span]
    /// Publish a canonical, atomically admitted batch of envelopes.
    ///
    /// Parsing derives topics and hashes before validation. Duplicate lookup
    /// then permits known rows to bypass validation, while the locked database
    /// pass still handles duplicates that race with validation. New rows and
    /// their identity projections commit together or not at all.
    async fn publish(
        &self,
        request: Request<api::PublishRequest>,
    ) -> Result<Response<api::PublishResponse>, Status> {
        let request = request.into_inner();
        let attempt = crate::telemetry::PublishAttempt::new(request.envelopes.len());
        let result = self.publish_batch(request).await;
        if let Ok((response, stored, duplicate)) = &result
            && response.get_ref().encoded_len() <= self.config.limits.max_response_bytes
        {
            attempt.succeeded(*stored, *duplicate);
        }
        // Tonic rejects oversized responses after commit. The guard counts them as rejected.
        result.map(|(response, _, _)| response)
    }
}

impl Backend {
    /// Keep response origins until the whole request succeeds or fails.
    async fn publish_batch(
        &self,
        request: api::PublishRequest,
    ) -> Result<(Response<api::PublishResponse>, usize, usize), Status> {
        let PublishBatch {
            mut pending,
            parsed,
            positions,
            parse_error,
        } = self.parse_publish(request)?;
        self.validate_publish(&mut pending, &parsed).await?;
        for (item, parsed) in pending.iter_mut().zip(parsed) {
            item.payload = parsed.canonical.bytes;
        }
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
        let duplicate = positions
            .iter()
            .filter(|&&position| pending[position].duplicate.is_some())
            .count();
        let stored = positions.len() - duplicate;
        let envelope_metas = positions
            .into_iter()
            .map(|position| metas[position].clone().into())
            .collect();
        Ok((
            Response::new(api::PublishResponse { envelope_metas }),
            stored,
            duplicate,
        ))
    }
}

impl Backend {
    /// Parse request limits, derive routing, and retain original positions.
    ///
    /// This step performs no cryptographic or identity validation. It collapses
    /// identical canonical envelopes, records the first parse error, and keeps
    /// enough metadata for the database layer to restore response order.
    #[xmtp_common::span(prefix = "publish")]
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
        let mut parsed_envelopes = Vec::new();
        let mut positions = Vec::with_capacity(request.envelopes.len());
        let mut parse_error = None;
        for (index, envelope) in request.envelopes.into_iter().enumerate() {
            if envelope.encoded_len() > limits.max_envelope_bytes {
                parse_error.get_or_insert((
                    index,
                    AdmissionError::TooLarge("envelope exceeds byte limit"),
                ));
                continue;
            }
            let parsed = match parse_envelope(envelope) {
                Ok(parsed) => parsed,
                Err(error) => {
                    parse_error.get_or_insert((index, AdmissionError::from(error)));
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
                    AdmissionError::InvalidIdentity("distinct updates address one inbox"),
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
                topic: parsed.topic.to_vec(),
                message_hash: parsed.canonical.hash,
                payload: Vec::new(),
                is_commit_or_proposal: parsed.is_commit_or_proposal,
                identity: (parsed.topic.kind() == TopicKind::IdentityUpdatesV1).then(|| {
                    IdentityAdmission {
                        inbox_id: parsed.topic.identifier().to_vec(),
                        head: 0,
                    }
                }),
                index,
                duplicate: None,
                validation: Ok(None),
                retention_ns,
            });
            parsed_envelopes.push(parsed);
        }
        Ok(PublishBatch {
            pending,
            parsed: parsed_envelopes,
            positions,
            parse_error,
        })
    }

    /// Validate only pending envelopes that are not known duplicates.
    ///
    /// Identity updates use one complete primary history snapshot and retain
    /// its head for the locked comparison during commit. Other payload kinds
    /// use shared structural validation, and verifier retryability is preserved
    /// for transport mapping.
    #[xmtp_common::span(prefix = "publish")]
    async fn validate_publish(
        &self,
        pending: &mut [PendingEnvelope],
        parsed: &[ParsedEnvelope],
    ) -> Result<(), Status> {
        self.store.find_duplicates(pending).await?;
        for (item, parsed) in pending.iter_mut().zip(parsed) {
            if item.duplicate.is_some() {
                continue;
            }
            let history = if let Some(Payload::IdentityUpdate(update)) = &parsed.envelope.payload {
                let history = self.store.history(&item.topic).await?;
                item.identity
                    .as_mut()
                    .ok_or_else(|| Status::internal("identity metadata missing"))?
                    .head = history.head;
                if scw_count(update) > self.config.limits.max_scw_signatures {
                    item.validation = Err(AdmissionError::TooLarge(
                        "identity update exceeds signature limit",
                    ));
                    continue;
                }
                if history.payloads.len() >= self.config.limits.max_identity_entries {
                    item.validation = Err(AdmissionError::InvalidIdentity(
                        "identity history limit reached",
                    ));
                    continue;
                }
                history
                    .payloads
                    .into_iter()
                    .map(|payload| {
                        match api::ClientEnvelope::decode(payload.as_slice())
                            .map_err(|_| Status::internal("stored identity envelope is invalid"))?
                            .payload
                        {
                            Some(Payload::IdentityUpdate(update)) => Ok(update),
                            _ => Err(Status::internal(
                                "identity history contains another payload kind",
                            )),
                        }
                    })
                    .collect::<Result<Vec<_>, Status>>()?
            } else {
                Vec::new()
            };
            item.validation = validate_envelope(
                parsed,
                &history,
                crate::validation::ObservedVerifier(&self.verifier),
            )
            .await
            .map(|result| result.as_ref().map(projection))
            .map_err(AdmissionError::from);
        }
        Ok(())
    }
}
