//! Bounded terminal causes belong to an intent, not to the latest topic error.

use super::*;
use serde::{Deserialize, Serialize};

/// Stored with the exact attempt and committed in the same writer as intent failure.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PreparedRejection {
    /// Authenticated cursor of the rejected own envelope.
    sequence_id: u64,
    /// A closed set of codes. Raw messages, credentials, and verifier data are not stored.
    code: RejectionCode,
}

/// Bincode stores variant positions. Append new codes; do not reorder existing codes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum RejectionCode {
    InsufficientPermissions,
    ActorCouldNotBeFound,
    ActorNotMember,
    SubjectDoesNotExist,
    MultipleActors,
    MissingGroupMembership,
    MissingMutableMetadata,
    SequenceIdDecreased,
    NoPskSupport,
    ProposerNotFound,
    ProposalsNotEnabled,
    IntentAlreadyProcessed,
    InvalidPayload,
    OwnMessageWithoutAttempt,
    InvalidSender,
    MalformedEnvelope,
    InvalidMlsInput,
    WrongCredentialType,
    UnsupportedMessageType,
    FutureEpoch,
    MessageAlreadyProcessed,
    IdentityReference,
    IdentitySequenceOrder,
    InboxValidation,
    InvalidVersion,
    UnexpectedInstallationAdded,
    UnexpectedInstallationsRemoved,
    GroupMetadata,
    MlsCredential,
    GroupMutableMetadata,
    GroupMutablePermissions,
    TooManyCharacters,
    MinVersionDowngrade,
    MinVersionRemove,
    ComponentSource,
    Bootstrap,
    Conversion,
    OtherSupportedInput,
}

impl RejectionCode {
    fn capture(error: &GroupMessageProcessingError) -> Self {
        use CommitValidationError as C;
        use GroupMessageProcessingError as G;
        match error {
            G::CommitValidation(error) => match error {
                C::InsufficientPermissions => Self::InsufficientPermissions,
                C::ActorCouldNotBeFound => Self::ActorCouldNotBeFound,
                C::ActorNotMember => Self::ActorNotMember,
                C::SubjectDoesNotExist => Self::SubjectDoesNotExist,
                C::MultipleActors => Self::MultipleActors,
                C::MissingGroupMembership => Self::MissingGroupMembership,
                C::MissingMutableMetadata => Self::MissingMutableMetadata,
                C::SequenceIdDecreased => Self::SequenceIdDecreased,
                C::NoPSKSupport => Self::NoPskSupport,
                C::ProposerNotFound => Self::ProposerNotFound,
                C::ProposalsNotEnabled => Self::ProposalsNotEnabled,
                C::IdentityDependency(_) => Self::IdentityReference,
                C::IdentitySequenceNotBeforeEnvelope { .. } => Self::IdentitySequenceOrder,
                C::InboxValidationFailed(_) => Self::InboxValidation,
                C::InvalidVersionFormat(_) => Self::InvalidVersion,
                C::UnexpectedInstallationAdded(_) => Self::UnexpectedInstallationAdded,
                C::UnexpectedInstallationsRemoved(_) => Self::UnexpectedInstallationsRemoved,
                C::GroupMetadata(_) => Self::GroupMetadata,
                C::MlsCredential(_) => Self::MlsCredential,
                C::GroupMutableMetadata(_) => Self::GroupMutableMetadata,
                C::GroupMutablePermissions(_) => Self::GroupMutablePermissions,
                C::TooManyCharacters { .. } => Self::TooManyCharacters,
                C::MinVersionDowngrade { .. } => Self::MinVersionDowngrade,
                C::MinVersionRemoveOnExistingFloor { .. } => Self::MinVersionRemove,
                C::ComponentSource(_) => Self::ComponentSource,
                C::Bootstrap(_) => Self::Bootstrap,
                C::Conversion(_) => Self::Conversion,
                C::ProtoDecode(_) => Self::MalformedEnvelope,
                _ => Self::OtherSupportedInput,
            },
            G::IntentAlreadyProcessed => Self::IntentAlreadyProcessed,
            G::InvalidPayload => Self::InvalidPayload,
            G::OwnMessageWithoutAttempt => Self::OwnMessageWithoutAttempt,
            G::InvalidSender { .. } => Self::InvalidSender,
            G::Envelope(_) | G::DecodeProto(_) | G::TlsError(_) => Self::MalformedEnvelope,
            G::OpenMlsProcessMessage(_) | G::OpenMlsProcessMessageWithAppData(_) => {
                Self::InvalidMlsInput
            }
            G::WrongCredentialType(_) => Self::WrongCredentialType,
            G::UnsupportedMessageType(_) => Self::UnsupportedMessageType,
            G::FutureEpoch(..) => Self::FutureEpoch,
            G::MessageAlreadyProcessed(_) => Self::MessageAlreadyProcessed,
            _ => Self::OtherSupportedInput,
        }
    }

    /// Reconstruct only parameter-free causes. Never invent discarded parameters.
    fn error(self) -> GroupMessageProcessingError {
        use CommitValidationError as C;
        use GroupMessageProcessingError as G;
        let code = match self {
            Self::InsufficientPermissions => {
                return G::CommitValidation(C::InsufficientPermissions);
            }
            Self::ActorCouldNotBeFound => return G::CommitValidation(C::ActorCouldNotBeFound),
            Self::ActorNotMember => return G::CommitValidation(C::ActorNotMember),
            Self::SubjectDoesNotExist => return G::CommitValidation(C::SubjectDoesNotExist),
            Self::MultipleActors => return G::CommitValidation(C::MultipleActors),
            Self::MissingGroupMembership => return G::CommitValidation(C::MissingGroupMembership),
            Self::MissingMutableMetadata => return G::CommitValidation(C::MissingMutableMetadata),
            Self::SequenceIdDecreased => return G::CommitValidation(C::SequenceIdDecreased),
            Self::NoPskSupport => return G::CommitValidation(C::NoPSKSupport),
            Self::ProposerNotFound => return G::CommitValidation(C::ProposerNotFound),
            Self::ProposalsNotEnabled => return G::CommitValidation(C::ProposalsNotEnabled),
            Self::IntentAlreadyProcessed => return G::IntentAlreadyProcessed,
            Self::InvalidPayload => return G::InvalidPayload,
            Self::OwnMessageWithoutAttempt => return G::OwnMessageWithoutAttempt,
            Self::InvalidSender => "invalid_sender",
            Self::MalformedEnvelope => "malformed_envelope",
            Self::InvalidMlsInput => "invalid_mls_input",
            Self::WrongCredentialType => "wrong_credential_type",
            Self::UnsupportedMessageType => "unsupported_message_type",
            Self::FutureEpoch => "impossible_future_epoch",
            Self::MessageAlreadyProcessed => "message_already_processed",
            Self::IdentityReference => "invalid_identity_reference",
            Self::IdentitySequenceOrder => "identity_sequence_order",
            Self::InboxValidation => "inbox_validation",
            Self::InvalidVersion => "invalid_version",
            Self::UnexpectedInstallationAdded => "unexpected_installation_added",
            Self::UnexpectedInstallationsRemoved => "unexpected_installations_removed",
            Self::GroupMetadata => "group_metadata",
            Self::MlsCredential => "mls_credential",
            Self::GroupMutableMetadata => "group_mutable_metadata",
            Self::GroupMutablePermissions => "group_mutable_permissions",
            Self::TooManyCharacters => "too_many_characters",
            Self::MinVersionDowngrade => "min_version_downgrade",
            Self::MinVersionRemove => "min_version_remove",
            Self::ComponentSource => "component_source",
            Self::Bootstrap => "bootstrap_validation",
            Self::Conversion => "conversion",
            Self::OtherSupportedInput => "supported_input_rejected",
        };
        G::RejectedIntent(code)
    }
}

impl<Context: XmtpSharedContext> MlsGroup<Context> {
    /// The caller holds the writer that will mark this intent Error and complete P.
    pub(in crate::groups::mls_sync) fn record_intent_rejection(
        db: &impl DbQuery,
        intent: &StoredGroupIntent,
        envelope: &GroupMessage,
        error: &GroupMessageProcessingError,
    ) -> Result<(), GroupMessageProcessingError> {
        let record = || -> Result<(), GroupError> {
            let encoded = db
                .prepared_envelopes(intent.id)?
                .ok_or(OutgoingPreparationError::MissingPreparedAttempt(intent.id))?;
            let mut attempt = PreparedAttempt::decode(&encoded)?;
            attempt.validate_intent(intent)?;
            if attempt.payload_hash != envelope.payload_hash || !error.is_safe_rejection() {
                return Err(OutgoingPreparationError::InvalidPreparedAttempt.into());
            }
            attempt.rejection = Some(PreparedRejection {
                sequence_id: envelope.cursor.0,
                code: RejectionCode::capture(error),
            });
            let replacement = xmtp_db::db_serialize(&attempt)?;
            if !db.compare_and_set_prepared_envelopes(
                intent.id,
                Some(&encoded),
                Some(&replacement),
            )? {
                return Err(OutgoingPreparationError::StateChanged.into());
            }
            Ok(())
        };
        record().map_err(|error| GroupMessageProcessingError::PreparedAttempt(Box::new(error)))
    }

    /// Read the failed attempt under one writer, including after client reconstruction.
    pub(in crate::groups::mls_sync) fn rejected_intent_summary(
        &self,
        intent_id: i32,
    ) -> Result<SyncSummary, GroupError> {
        crate::state_tx::state_write(self.context.mls_storage(), |tx| {
            let storage = tx.storage();
            let db = storage.db();
            let intent = Fetch::<StoredGroupIntent>::fetch(&db, &intent_id)?
                .ok_or(NotFound::IntentById(intent_id))?;
            if intent.group_id != self.group_id || intent.state != IntentState::Error {
                return Err(OutgoingPreparationError::StateChanged.into());
            }
            let rejection = match db.prepared_envelopes(intent_id)? {
                Some(encoded) => {
                    let attempt = PreparedAttempt::decode(&encoded)?;
                    attempt.validate_intent(&intent)?;
                    attempt.rejection
                }
                None => None,
            };
            let Some(rejection) = rejection else {
                return Ok(Continue(SyncSummary::other(GroupError::ReceiveError(
                    GroupMessageProcessingError::RejectedIntent("rejection_details_unavailable"),
                ))));
            };
            if rejection.sequence_id == 0 || rejection.sequence_id > i64::MAX as u64 {
                return Err(OutgoingPreparationError::InvalidPreparedAttempt.into());
            }
            let mut summary = SyncSummary::other(GroupError::ReceiveError(rejection.code.error()));
            let cursor = Cursor(rejection.sequence_id);
            summary.process.add_id(cursor);
            summary
                .process
                .errored
                .push((cursor, rejection.code.error()));
            Ok::<_, GroupError>(Continue(summary))
        })
        .map(TransactionOutcome::into_continued)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[xmtp_common::test(unwrap_try = true)]
    fn stored_rejections_preserve_exact_unit_causes_without_private_error_data() {
        use CommitValidationError as C;
        use GroupMessageProcessingError as G;
        for error in [
            C::InsufficientPermissions,
            C::ActorCouldNotBeFound,
            C::ActorNotMember,
            C::SubjectDoesNotExist,
            C::MultipleActors,
            C::MissingGroupMembership,
            C::MissingMutableMetadata,
            C::SequenceIdDecreased,
            C::NoPSKSupport,
            C::ProposerNotFound,
            C::ProposalsNotEnabled,
        ] {
            let original = std::mem::discriminant(&error);
            let saved = PreparedRejection {
                sequence_id: 17,
                code: RejectionCode::capture(&G::CommitValidation(error)),
            };
            let bytes = xmtp_db::db_serialize(&saved)?;
            assert_eq!(bytes.len(), 12);
            let restored: PreparedRejection = xmtp_db::db_deserialize(&bytes)?;
            let G::CommitValidation(error) = restored.code.error() else {
                panic!("parameter-free commit rejection lost its typed cause");
            };
            assert_eq!(std::mem::discriminant(&error), original);
        }
        let code = RejectionCode::capture(&G::CommitValidation(C::InboxValidationFailed(
            "private credential data".into(),
        )));
        let bytes = xmtp_db::db_serialize(&code)?;
        assert_eq!(bytes.len(), 4);
        assert!(matches!(
            code.error(),
            G::RejectedIntent("inbox_validation")
        ));
    }
}
