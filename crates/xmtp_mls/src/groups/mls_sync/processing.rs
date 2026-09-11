//! One synchronous attempt at the durable group head.

use super::*;
use crate::{
    identity_updates::{IdentityDependencyError, IdentityRequirement},
    state_tx::state_write,
};
use xmtp_api_backend::envelope::decode_group_message;
use xmtp_db::incoming_envelope::{
    IncomingRetry, QueryIncomingEnvelope, StoredIncomingEnvelope, StreamTopic,
};
use xmtp_proto::backend_v1::ServerEnvelope;

/// Local attempt result and diagnostics for the ordered group processor.
pub(crate) enum GroupHeadOutcome {
    Idle,
    Inactive,
    Waiting {
        cursor: Cursor,
        code: String,
        blocked: bool,
    },
    Need {
        cursor: Cursor,
        requirement: IdentityRequirement,
    },
    Progress {
        cursor: Cursor,
        result: Result<ProcessedMessageOutcome, GroupMessageProcessingError>,
    },
}

impl<Context: XmtpSharedContext> MlsGroup<Context> {
    /// Attempt one head. Network dependencies are returned to the scheduler.
    #[cfg(test)]
    pub(crate) fn process_pending_group_head(
        &self,
        missing_reference: Option<&IdentityRequirement>,
    ) -> Result<GroupHeadOutcome, GroupMessageProcessingError> {
        self.process_pending_group_head_with_retry(missing_reference, false)
    }

    pub(crate) fn process_pending_group_head_with_retry(
        &self,
        missing_reference: Option<&IdentityRequirement>,
        retry_blocked: bool,
    ) -> Result<GroupHeadOutcome, GroupMessageProcessingError> {
        let topic = StreamTopic::group(self.group_id);
        if group_is_restored(&self.context.db(), &self.group_id)? {
            return Ok(GroupHeadOutcome::Inactive);
        }
        let Some(pending) = self.context.db().first_pending_envelope(&topic)? else {
            return Ok(GroupHeadOutcome::Idle);
        };
        let cursor = Cursor(pending.sequence_id as u64);
        if pending.blocked && !retry_blocked || !pending.blocked && pending.retry_at_ns > now_ns() {
            return Ok(GroupHeadOutcome::Waiting {
                cursor,
                code: pending
                    .error_code
                    .unwrap_or_else(|| "processing_retry".into()),
                blocked: pending.blocked,
            });
        }

        let wire = ServerEnvelope::decode(pending.envelope.as_slice())
            .map_err(GroupMessageProcessingError::CorruptIncomingEnvelope);
        let unsupported = wire
            .as_ref()
            .ok()
            .and_then(|wire| wire.envelope.as_ref())
            .and_then(|envelope| envelope.payload.as_ref())
            .and_then(|payload| match payload {
                Payload::GroupMessage(message) => message.data.get(..2),
                _ => None,
            })
            .is_some_and(|version| version != [0, 1]);
        let result = if unsupported {
            Err(GroupMessageProcessingError::UnsupportedMlsVersion)
        } else {
            match wire.map(decode_group_message) {
                Err(error) => Err(error),
                Ok(Ok(envelope)) => {
                    self.apply_pending_group_envelope(&pending, &envelope, missing_reference)
                }
                Ok(Err(error)) => self.reject_malformed_group_envelope(&pending, error),
            }
        };
        match result {
            Ok(outcome) => Ok(GroupHeadOutcome::Progress {
                cursor,
                result: Ok(outcome),
            }),
            Err(GroupMessageProcessingError::GroupInactive) => Ok(GroupHeadOutcome::Inactive),
            Err(GroupMessageProcessingError::IncomingHeadChanged) => Ok(GroupHeadOutcome::Idle),
            Err(GroupMessageProcessingError::CommitValidation(
                CommitValidationError::IdentityDependency(IdentityDependencyError::Need(
                    requirement,
                )),
            )) => {
                self.context.db().set_incoming_retry(
                    &topic,
                    cursor,
                    &IncomingRetry {
                        retry_at_ns: 0,
                        blocked: false,
                        error_code: Some("identity_dependency".into()),
                        retry_expires_at_ns: None,
                    },
                )?;
                Ok(GroupHeadOutcome::Need {
                    cursor,
                    requirement,
                })
            }
            Err(error) if error.is_safe_rejection() => Ok(GroupHeadOutcome::Progress {
                cursor,
                result: Err(error),
            }),
            Err(error) => {
                let blocked = !error.is_retryable();
                let code = error.processing_code();
                let delay = self.context.stream_settings().active_database_poll_interval;
                let retry_at_ns = now_ns().saturating_add(delay.as_nanos() as i64);
                self.context.db().set_incoming_retry(
                    &topic,
                    cursor,
                    &IncomingRetry {
                        retry_at_ns,
                        blocked,
                        error_code: Some(code.into()),
                        retry_expires_at_ns: None,
                    },
                )?;
                tracing::warn!(group_id = %self.group_id, sequence_id = cursor.0, code, "group head remains pending");
                Ok(GroupHeadOutcome::Waiting {
                    cursor,
                    code: code.into(),
                    blocked,
                })
            }
        }
    }

    fn apply_pending_group_envelope(
        &self,
        pending: &StoredIncomingEnvelope,
        envelope: &GroupMessage,
        missing_reference: Option<&IdentityRequirement>,
    ) -> Result<ProcessedMessageOutcome, GroupMessageProcessingError> {
        let topic = StreamTopic::group(self.group_id);
        let watch_app_data = self.context.change_callbacks().watches_app_data();
        let mut events = DeferredEvents::new();
        let result = state_write(self.context.mls_storage(), |tx| {
            check_current_head(&tx.storage().db(), &topic, pending)?;
            if group_is_restored(&tx.storage().db(), &self.group_id)? {
                return Err(GroupMessageProcessingError::GroupInactive);
            }
            let attempt = tx.savepoint(|tx| {
                tx.with_group(self.group_id, |group, storage| {
                    if !group.is_active() {
                        return Err(GroupMessageProcessingError::GroupInactive);
                    }
                    if envelope.group_id != self.group_id {
                        return Err(GroupMessageProcessingError::InvalidPayload);
                    }
                    if !matches!(envelope.message, ProtocolMessage::PrivateMessage(_)) {
                        return Err(GroupMessageProcessingError::UnsupportedMessageType(
                            discriminant(&envelope.message),
                        ));
                    }
                    let before = watch_app_data
                        .then(|| Self::read_app_data_slot(group))
                        .flatten();
                    let mut outcome =
                        self.process_message_inner(group, storage, envelope, &mut events)?;
                    if watch_app_data
                        && let (Some(before), Some(after)) =
                            (before, Self::read_app_data_slot(group))
                        && before != after
                    {
                        outcome.app_data_change = Some(AppDataChange {
                            group_id: self.group_id.to_vec(),
                            old_value: before,
                            new_value: after,
                        });
                    }
                    storage
                        .db()
                        .complete_pending_envelope(&topic, envelope.cursor)?;
                    Ok(outcome)
                })
                .map(Continue)
            });
            let error = match attempt {
                Ok(Continue(outcome)) => return Ok(Continue(Ok(outcome))),
                Ok(Rollback) => unreachable!("processing does not request a rollback"),
                Err(error) => error,
            };
            events = DeferredEvents::new();
            let error = match (&error, missing_reference) {
                (
                    GroupMessageProcessingError::CommitValidation(
                        CommitValidationError::IdentityDependency(IdentityDependencyError::Need(
                            required,
                        )),
                    ),
                    Some(missing),
                ) if required == missing => GroupMessageProcessingError::CommitValidation(
                    CommitValidationError::IdentityDependency(
                        IdentityDependencyError::MissingReference(missing.clone()),
                    ),
                ),
                _ => error,
            };
            if let GroupMessageProcessingError::CommitValidation(
                CommitValidationError::ProtocolVersionTooLow(version),
            ) = &error
            {
                tx.storage()
                    .db()
                    .set_group_paused(&self.group_id, version)?;
                return Ok(Continue(Err(GroupMessageProcessingError::GroupPaused)));
            }
            if error.is_safe_rejection() {
                tx.with_group(self.group_id, |group, storage| {
                    self.record_rejected_message(group, storage, envelope, &error)?;
                    storage.db().record_terminal_rejection(
                        &topic,
                        envelope.cursor,
                        error.processing_code(),
                    )?;
                    storage
                        .db()
                        .complete_pending_envelope(&topic, envelope.cursor)?;
                    Ok::<_, GroupMessageProcessingError>(())
                })?;
                return Ok(Continue(Err(error)));
            }
            Err(error)
        })
        .map(TransactionOutcome::into_continued)
        .and_then(|outcome| outcome);
        if let Ok(outcome) = &result {
            events.send_all(&self.context);
            if outcome.disappearing_message_stored
                && self
                    .context
                    .worker_config()
                    .worker_enabled(WorkerKind::DisappearingMessages)
            {
                self.context.disappearing_channels().rearm();
            }
        }
        result
    }

    fn reject_malformed_group_envelope(
        &self,
        pending: &StoredIncomingEnvelope,
        error: xmtp_api_backend::envelope::EnvelopeError,
    ) -> Result<ProcessedMessageOutcome, GroupMessageProcessingError> {
        let topic = StreamTopic::group(self.group_id);
        let cursor = Cursor(pending.sequence_id as u64);
        let error = GroupMessageProcessingError::Envelope(error);
        state_write(self.context.mls_storage(), |tx| {
            check_current_head(&tx.storage().db(), &topic, pending)?;
            if group_is_restored(&tx.storage().db(), &self.group_id)? {
                return Err(GroupMessageProcessingError::GroupInactive);
            }
            tx.with_group(self.group_id, |group, storage| {
                if !group.is_active() {
                    return Err(GroupMessageProcessingError::GroupInactive);
                }
                storage
                    .db()
                    .record_terminal_rejection(&topic, cursor, error.processing_code())?;
                storage.db().complete_pending_envelope(&topic, cursor)?;
                Ok::<_, GroupMessageProcessingError>(())
            })?;
            Ok::<_, GroupMessageProcessingError>(Continue(()))
        })?;
        Err(error)
    }
}

/// Archive placeholders have no joined state, even when their local MLS group is active.
fn group_is_restored(
    db: &impl DbQuery,
    group_id: &GroupId,
) -> Result<bool, GroupMessageProcessingError> {
    Ok(db
        .find_group(group_id)?
        .is_some_and(|group| group.membership_state == GroupMembershipState::Restored))
}

fn check_current_head(
    db: &impl DbQuery,
    topic: &StreamTopic,
    expected: &StoredIncomingEnvelope,
) -> Result<(), GroupMessageProcessingError> {
    if db.first_pending_envelope(topic)?.is_some_and(|head| {
        head.sequence_id == expected.sequence_id && head.envelope == expected.envelope
    }) {
        Ok(())
    } else {
        Err(GroupMessageProcessingError::IncomingHeadChanged)
    }
}
