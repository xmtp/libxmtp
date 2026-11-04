//! Bounded terminal causes belong to an intent, not to the latest topic error.

use super::*;
use serde::{Deserialize, Serialize};
use xmtp_mls_validation::commit::CommitRuleError;

/// Stored with the exact attempt and committed in the same writer as intent failure.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PreparedRejection {
    /// Authenticated cursor of the rejected own envelope.
    sequence_id: u64,
    /// A closed set of codes. Raw messages, credentials, and verifier data are not stored.
    code: RejectionCode,
}

/// The stored row names each code (CBOR stores the variant name). Never rename
/// or remove a code: stored rows would stop decoding. Adding a code is safe.
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

#[cfg(test)]
impl PreparedRejection {
    /// A rejection with a fixed cursor and code, for stored-format tests.
    pub(super) fn sample() -> Self {
        Self {
            sequence_id: 42,
            code: RejectionCode::InboxValidation,
        }
    }
}

impl RejectionCode {
    fn capture(error: &GroupMessageProcessingError) -> Self {
        use CommitRuleError as C;
        use CommitValidationError as V;
        use GroupMessageProcessingError as G;
        match error {
            G::CommitValidation(V::IdentityDependency(_)) => Self::IdentityReference,
            G::CommitValidation(V::Rule(error)) => match error {
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
                C::IdentitySequenceNotBeforeEnvelope { .. } => Self::IdentitySequenceOrder,
                C::InboxValidationFailed(_) => Self::InboxValidation,
                C::InvalidVersionFormat(_) => Self::InvalidVersion,
                C::UnexpectedInstallationAdded(_) => Self::UnexpectedInstallationAdded,
                C::UnexpectedInstallationsRemoved(_) => Self::UnexpectedInstallationsRemoved,
                C::GroupMetadata(_) => Self::GroupMetadata,
                C::MlsCredential(_) => Self::MlsCredential,
                C::GroupMutableMetadata(_) => Self::GroupMutableMetadata,
                C::TooManyCharacters { .. } => Self::TooManyCharacters,
                C::MinVersionDowngrade { .. } => Self::MinVersionDowngrade,
                C::MinVersionRemoveOnExistingFloor { .. } => Self::MinVersionRemove,
                C::ComponentSource(_) => Self::ComponentSource,
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
        use CommitRuleError as C;
        use GroupMessageProcessingError as G;
        let code = match self {
            Self::InsufficientPermissions => {
                return G::CommitValidation(C::InsufficientPermissions.into());
            }
            Self::ActorCouldNotBeFound => {
                return G::CommitValidation(C::ActorCouldNotBeFound.into());
            }
            Self::ActorNotMember => return G::CommitValidation(C::ActorNotMember.into()),
            Self::SubjectDoesNotExist => return G::CommitValidation(C::SubjectDoesNotExist.into()),
            Self::MultipleActors => return G::CommitValidation(C::MultipleActors.into()),
            Self::MissingGroupMembership => {
                return G::CommitValidation(C::MissingGroupMembership.into());
            }
            Self::MissingMutableMetadata => {
                return G::CommitValidation(C::MissingMutableMetadata.into());
            }
            Self::SequenceIdDecreased => return G::CommitValidation(C::SequenceIdDecreased.into()),
            Self::NoPskSupport => return G::CommitValidation(C::NoPSKSupport.into()),
            Self::ProposerNotFound => return G::CommitValidation(C::ProposerNotFound.into()),
            Self::ProposalsNotEnabled => return G::CommitValidation(C::ProposalsNotEnabled.into()),
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
            Self::fail_membership_parent(db, intent, &attempt)?;
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
            // Split parents have no attempt of their own while a child fails.
            // The child's durable code remains the original request's outcome,
            // including after reconstructing the client over the same store.
            let rejection = match rejection {
                Some(rejection) => Some(rejection),
                None => Self::membership_child_attempts(
                    &db,
                    self.group_id,
                    intent_id,
                    &[IntentState::Error],
                )?
                .into_iter()
                .filter_map(|(_, attempt)| attempt.rejection)
                .max_by_key(|rejection| rejection.sequence_id),
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

    /// A CBOR text string shorter than 256 bytes, built by hand.
    fn cbor_text(text: &str) -> Vec<u8> {
        let head = match text.len() {
            len @ 0..=23 => vec![0x60 | len as u8],
            len => vec![0x78, u8::try_from(len).unwrap()],
        };
        [head.as_slice(), text.as_bytes()].concat()
    }

    /// Stored rows name each code. Every name is written out here, so a
    /// renamed code fails this test instead of every stored row of an
    /// in-flight intent, and the list must match every code serde stores, so
    /// a new code fails until its name is here.
    #[xmtp_common::test(unwrap_try = true)]
    fn every_rejection_code_is_stored_by_its_name() {
        let codes = [
            (
                RejectionCode::InsufficientPermissions,
                "InsufficientPermissions",
            ),
            (RejectionCode::ActorCouldNotBeFound, "ActorCouldNotBeFound"),
            (RejectionCode::ActorNotMember, "ActorNotMember"),
            (RejectionCode::SubjectDoesNotExist, "SubjectDoesNotExist"),
            (RejectionCode::MultipleActors, "MultipleActors"),
            (
                RejectionCode::MissingGroupMembership,
                "MissingGroupMembership",
            ),
            (
                RejectionCode::MissingMutableMetadata,
                "MissingMutableMetadata",
            ),
            (RejectionCode::SequenceIdDecreased, "SequenceIdDecreased"),
            (RejectionCode::NoPskSupport, "NoPskSupport"),
            (RejectionCode::ProposerNotFound, "ProposerNotFound"),
            (RejectionCode::ProposalsNotEnabled, "ProposalsNotEnabled"),
            (
                RejectionCode::IntentAlreadyProcessed,
                "IntentAlreadyProcessed",
            ),
            (RejectionCode::InvalidPayload, "InvalidPayload"),
            (
                RejectionCode::OwnMessageWithoutAttempt,
                "OwnMessageWithoutAttempt",
            ),
            (RejectionCode::InvalidSender, "InvalidSender"),
            (RejectionCode::MalformedEnvelope, "MalformedEnvelope"),
            (RejectionCode::InvalidMlsInput, "InvalidMlsInput"),
            (RejectionCode::WrongCredentialType, "WrongCredentialType"),
            (
                RejectionCode::UnsupportedMessageType,
                "UnsupportedMessageType",
            ),
            (RejectionCode::FutureEpoch, "FutureEpoch"),
            (
                RejectionCode::MessageAlreadyProcessed,
                "MessageAlreadyProcessed",
            ),
            (RejectionCode::IdentityReference, "IdentityReference"),
            (
                RejectionCode::IdentitySequenceOrder,
                "IdentitySequenceOrder",
            ),
            (RejectionCode::InboxValidation, "InboxValidation"),
            (RejectionCode::InvalidVersion, "InvalidVersion"),
            (
                RejectionCode::UnexpectedInstallationAdded,
                "UnexpectedInstallationAdded",
            ),
            (
                RejectionCode::UnexpectedInstallationsRemoved,
                "UnexpectedInstallationsRemoved",
            ),
            (RejectionCode::GroupMetadata, "GroupMetadata"),
            (RejectionCode::MlsCredential, "MlsCredential"),
            (RejectionCode::GroupMutableMetadata, "GroupMutableMetadata"),
            (
                RejectionCode::GroupMutablePermissions,
                "GroupMutablePermissions",
            ),
            (RejectionCode::TooManyCharacters, "TooManyCharacters"),
            (RejectionCode::MinVersionDowngrade, "MinVersionDowngrade"),
            (RejectionCode::MinVersionRemove, "MinVersionRemove"),
            (RejectionCode::ComponentSource, "ComponentSource"),
            (RejectionCode::Bootstrap, "Bootstrap"),
            (RejectionCode::Conversion, "Conversion"),
            (RejectionCode::OtherSupportedInput, "OtherSupportedInput"),
        ];
        assert_eq!(
            crate::groups::mls_sync::publish::tests::serde_variants::<RejectionCode>(),
            codes.map(|(_, name)| name)
        );
        for (code, name) in codes {
            assert_eq!(xmtp_db::db_serialize(&code)?, cbor_text(name), "{name}");
            assert_eq!(
                xmtp_db::db_deserialize::<RejectionCode>(&cbor_text(name))?,
                code
            );
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn stored_rejections_preserve_exact_unit_causes_without_private_error_data() {
        use CommitRuleError as C;
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
                code: RejectionCode::capture(&G::CommitValidation(error.into())),
            };
            let bytes = xmtp_db::db_serialize(&saved)?;
            // map(2) "sequence_id" 17 "code" <the variant name>
            let name = format!("{:?}", saved.code);
            let expected = [
                &[0xa2, 0x6b][..],
                b"sequence_id",
                &[0x11, 0x64],
                b"code",
                &cbor_text(&name),
            ]
            .concat();
            assert_eq!(bytes, expected);
            let restored: PreparedRejection = xmtp_db::db_deserialize(&bytes)?;
            let G::CommitValidation(CommitValidationError::Rule(error)) = restored.code.error()
            else {
                panic!("parameter-free commit rejection lost its typed cause");
            };
            assert_eq!(std::mem::discriminant(&error), original);
        }
        let code = RejectionCode::capture(&G::CommitValidation(
            C::InboxValidationFailed("private credential data".into()).into(),
        ));
        let bytes = xmtp_db::db_serialize(&code)?;
        assert_eq!(bytes, cbor_text("InboxValidation"));
        assert!(matches!(
            code.error(),
            G::RejectedIntent("inbox_validation")
        ));
    }
}
