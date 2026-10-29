use super::error::publish_invalid;
use crate::{
    Backend,
    api::{self, client_envelope::Payload, publish_error::Reason},
    db::{History, IdentityAdmission, PendingEnvelope},
    error::AdmissionError,
    handles,
    validation::{PublishChainReads, projection},
};
use prost::Message;
use std::collections::{HashMap, HashSet};
use tonic::{Request, Response, Status};
use xmtp_common::time::{Duration, timeout};
use xmtp_configuration::{MAX_PUBLISH_ENVELOPES, MAX_PUBLISH_IDENTITY_UPDATES};
use xmtp_id::associations::check_update_shape;
use xmtp_mls_validation::{
    AdmittedState, ParsedEnvelope, ValidationError, admitted_state, erc6492_signatures,
    parse_envelope, validate_envelope, verify_publish,
};
use xmtp_proto::types::TopicKind;

#[cfg(test)]
mod tests;

/// The longest admission may take, before the commit timer starts.
const ADMISSION_DEADLINE: Duration = Duration::from_secs(60);
/// Identity-log replays running at once, across all requests.
pub(crate) const MAX_IDENTITY_REPLAYS: usize = 64;

/// Counts publish-signature verification work, so tests can check where and
/// how often it runs.
#[cfg(test)]
#[derive(Debug, Default)]
pub(crate) struct SignatureChecks {
    /// Blocking tasks that verified a request's signatures off the thread
    /// that started them. A verification run inline on the async runtime is
    /// not counted.
    tasks: std::sync::atomic::AtomicUsize,
    /// Signatures verified.
    verified: std::sync::atomic::AtomicUsize,
}

#[cfg(test)]
impl SignatureChecks {
    /// The blocking tasks started and the signatures verified since the last
    /// call.
    pub(crate) fn take(&self) -> (usize, usize) {
        use std::sync::atomic::Ordering::SeqCst;
        (self.tasks.swap(0, SeqCst), self.verified.swap(0, SeqCst))
    }
}

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
    // implements: API-221, API-290
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
        let parsed = timeout(
            self.admission_deadline(),
            self.validate_publish(&mut pending, parsed),
        )
        .await
        .map_err(|_| Status::deadline_exceeded("publish admission timed out"))??;
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
    /// [`ADMISSION_DEADLINE`], unless a test replaced it.
    fn admission_deadline(&self) -> Duration {
        #[cfg(test)]
        if let Some(deadline) = *self.admission_deadline.lock() {
            return deadline;
        }
        ADMISSION_DEADLINE
    }

    /// Parse request limits, derive routing, and retain original positions.
    ///
    /// This step performs no cryptographic or identity validation. It collapses
    /// identical canonical envelopes, records the first parse error, and keeps
    /// enough metadata for the database layer to restore response order. The
    /// envelope bound counts the input, duplicates included, before any
    /// envelope is parsed, so it also caps the signatures a request can make
    /// the backend verify. The identity update bound caps the stored logs one
    /// request replays.
    #[xmtp_common::span(prefix = "publish")]
    // implements: API-233, API-223
    fn parse_publish(&self, request: api::PublishRequest) -> Result<PublishBatch, Status> {
        let limits = &self.config.limits;
        if request.encoded_len() > limits.max_request_bytes {
            return Err(publish_invalid(
                None,
                Reason::TooLarge,
                "publish request exceeds byte limit",
            ));
        }
        if request.envelopes.len() > MAX_PUBLISH_ENVELOPES {
            return Err(publish_invalid(
                None,
                Reason::TooLarge,
                "publish request exceeds envelope limit",
            ));
        }
        let identity_updates = request
            .envelopes
            .iter()
            .filter(|envelope| matches!(envelope.payload, Some(Payload::IdentityUpdate(_))))
            .count();
        if identity_updates > MAX_PUBLISH_IDENTITY_UPDATES {
            return Err(publish_invalid(
                None,
                Reason::TooLarge,
                "publish request exceeds identity update limit",
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
            let (push_eligible, sender_hmac) = parsed.push_fields();
            pending.push(PendingEnvelope {
                push_eligible,
                sender_hmac,
                topic: parsed.topic.to_vec(),
                message_hash: parsed.canonical.hash,
                payload: Vec::new(),
                is_commit_or_proposal: parsed.is_commit_or_proposal,
                identity: (parsed.topic.kind() == TopicKind::IdentityUpdatesV1).then(|| {
                    IdentityAdmission {
                        inbox_id: parsed.topic.identifier().to_vec(),
                        head: None,
                        claim: None,
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
    /// An identity update first passes the checks that need no history, its
    /// ERC-6492 signature count and [`check_update_shape`]; one that fails
    /// them is rejected without reading its inbox's history and keeps no
    /// head, so the commit reports its own rejection and never a stale
    /// history. Every other identity update uses one complete primary history
    /// snapshot, replayed without chain access by [`Self::replay_history`],
    /// and retains its head for the locked comparison during commit. Other
    /// payload kinds use shared structural validation, and verifier
    /// retryability is preserved for transport mapping. A claim that passes validation then passes the
    /// handle policy that needs no lock; the taken check runs during commit.
    /// Publish signatures are verified last, together, by
    /// [`Self::verify_signatures`], the only place that verifies them:
    /// [`validate_envelope`] does not. No failure stops validation of the
    /// envelopes after it. The parsed envelopes are returned for storage.
    #[xmtp_common::span(prefix = "publish")]
    // implements: API-234
    async fn validate_publish(
        &self,
        pending: &mut [PendingEnvelope],
        parsed: Vec<ParsedEnvelope>,
    ) -> Result<Vec<ParsedEnvelope>, Status> {
        self.store.find_duplicates(pending).await?;
        let chain = PublishChainReads::new(&*self.chains);
        for (item, parsed) in pending.iter_mut().zip(&parsed) {
            if item.duplicate.is_some() {
                continue;
            }
            let mut stored_claims = Vec::new();
            let mut prior = AdmittedState::default();
            if let Some(Payload::IdentityUpdate(update)) = &parsed.envelope.payload {
                // implements: API-302
                if erc6492_signatures(update).count() > self.config.limits.max_scw_signatures {
                    item.validation = Err(AdmissionError::TooLarge(
                        "identity update exceeds signature limit",
                    ));
                    continue;
                }
                if let Err(error) = check_update_shape(update) {
                    item.validation = Err(ValidationError::from(error).into());
                    continue;
                }
                let history = self.store.history(&item.topic).await?;
                // implements: API-303
                item.identity
                    .as_mut()
                    .ok_or_else(|| Status::internal("identity metadata missing"))?
                    .head = Some(history.head);
                if history.payloads.len() >= self.config.limits.max_identity_entries {
                    item.validation = Err(AdmissionError::InvalidIdentity(
                        "identity history limit reached",
                    ));
                    continue;
                }
                match self.replay_history(history).await? {
                    Ok(replayed) => (stored_claims, prior) = replayed,
                    Err(error) => {
                        item.validation = Err(error.into());
                        continue;
                    }
                }
            }
            let validation = match validate_envelope(
                parsed,
                &prior,
                crate::validation::ObservedVerifier(&self.verifier),
                &chain,
            )
            .await
            {
                Ok(validation) => validation,
                Err(error) => {
                    item.validation = Err(error.into());
                    continue;
                }
            };
            item.validation = match validation {
                Some(validation) => match validation.claimed_handles.first() {
                    Some(handle) => {
                        let clock_ns = self.store.clock_ns().await?;
                        item.identity
                            .as_mut()
                            .ok_or_else(|| Status::internal("identity metadata missing"))?
                            .claim = Some(handle.clone());
                        handles::check_claim(
                            handle,
                            self.config.handles.min_name_length,
                            stored_claims,
                            clock_ns,
                        )
                        .map(|()| Some(projection(&validation)))
                        .map_err(AdmissionError::Handle)
                    }
                    None => Ok(Some(projection(&validation))),
                },
                None => Ok(None),
            };
        }
        self.verify_signatures(pending, parsed).await
    }

    /// Decode an inbox's stored identity log and replay it to the state a new
    /// update is validated against, inside one blocking task, with the
    /// `server_ns` of each stored update that claims a handle.
    ///
    /// The replay asks no chain about a stored ERC-6492 signature
    /// ([`admitted_state`]), and that is safe only because `history` is this
    /// backend's own primary snapshot. It is still CPU work, up to two
    /// signature checks for each stored action, so it runs off the async
    /// runtime. A stored row that is not an identity envelope is an internal
    /// error; a stored update that fails validation rejects the new update,
    /// as the inner error. It waits for one of [`MAX_IDENTITY_REPLAYS`]
    /// permits. A cancelled request leaves the task to finish, holding its
    /// permit; the entry limit bounds it at one log.
    // implements: IDENT-095
    async fn replay_history(
        &self,
        history: History,
    ) -> Result<Result<(Vec<i64>, AdmittedState), ValidationError>, Status> {
        let History {
            payloads,
            server_ns,
            ..
        } = history;
        if payloads.is_empty() {
            return Ok(Ok(Default::default()));
        }
        #[cfg(test)]
        let (replays, caller) = (self.identity_replays.clone(), std::thread::current().id());
        let permit = self
            .replays
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| Status::internal("identity replay limit closed"))?;
        let span = tracing::Span::current();
        tokio::task::spawn_blocking(move || {
            // Held until the replay ends, even after the request is dropped.
            let _permit = permit;
            let _span = span.enter();
            #[cfg(test)]
            if std::thread::current().id() != caller {
                replays.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
            let updates = payloads
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
                .collect::<Result<Vec<_>, Status>>()?;
            let claims = updates
                .iter()
                .zip(server_ns)
                .filter(|(update, _)| handles::carries_claim(update))
                .map(|(_, server_ns)| server_ns)
                .collect();
            Ok(admitted_state(updates).map(|prior| (claims, prior)))
        })
        .await
        .map_err(|_| Status::internal("identity replay task failed"))?
    }

    /// Verify the publish signature of every signed envelope that is not a
    /// known duplicate, inside one blocking task.
    ///
    /// It runs after the duplicate lookup, so a replay of stored envelopes
    /// costs nothing, and parsing collapsed identical envelopes, so each
    /// distinct envelope is verified at most once. It verifies every one, with
    /// no early stop: the locked duplicate check during commit can drop a
    /// failure that a concurrent request has meanwhile stored, and every
    /// envelope after a stop would then be stored unchecked. The parsed
    /// envelopes move into the task and back, so no payload is copied. A
    /// cancelled request leaves the task to finish; the envelope bound caps
    /// it at 1000 verifications.
    // implements: API-230, API-222
    async fn verify_signatures(
        &self,
        pending: &mut [PendingEnvelope],
        parsed: Vec<ParsedEnvelope>,
    ) -> Result<Vec<ParsedEnvelope>, Status> {
        let unchecked: Vec<usize> = pending
            .iter()
            .zip(&parsed)
            .enumerate()
            .filter(|(_, (item, parsed))| item.duplicate.is_none() && parsed.is_signed())
            .map(|(position, _)| position)
            .collect();
        if unchecked.is_empty() {
            return Ok(parsed);
        }
        #[cfg(test)]
        let (checks, caller) = (self.signature_checks.clone(), std::thread::current().id());
        let span = tracing::Span::current();
        let (parsed, results) = tokio::task::spawn_blocking(move || {
            let _span = span.enter();
            #[cfg(test)]
            if std::thread::current().id() != caller {
                checks
                    .tasks
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
            let results: Vec<_> = unchecked
                .into_iter()
                .map(|position| {
                    #[cfg(test)]
                    checks
                        .verified
                        .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    (position, verify_publish(&parsed[position]))
                })
                .collect();
            (parsed, results)
        })
        .await
        .map_err(|_| Status::internal("publish signature verification did not finish"))?;
        for (position, result) in results {
            if let Err(error) = result {
                pending[position].validation = Err(error.into());
            }
        }
        Ok(parsed)
    }
}
