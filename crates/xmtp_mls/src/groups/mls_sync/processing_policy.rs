//! Terminal rejection is a protocol decision, separate from retryability.

use super::GroupMessageProcessingError;
use crate::groups::app_data::ProcessMessageWithAppDataError;
use openmls::framing::errors::{MessageDecryptionError, SecretTreeError};
use openmls::group::{ProcessMessageError, StageCommitError, ValidationError};
use xmtp_db::sql_key_store::SqlKeyStoreError;

impl GroupMessageProcessingError {
    /// Stable diagnostics contain no ciphertext, credentials, or verifier text.
    pub(crate) fn processing_code(&self) -> &'static str {
        match self {
            Self::CorruptIncomingEnvelope(_) => "corrupt_incoming_envelope",
            Self::UnsupportedMlsVersion => "unsupported_mls_version",
            Self::GroupPaused => "unsupported_protocol_version",
            Self::GroupInactive => "group_inactive",
            Self::IncomingHeadChanged => "incoming_head_changed",
            Self::OwnMessageWithoutAttempt => "own_message_without_attempt",
            Self::UnsupportedOwnIntentKind(_) => "unsupported_own_intent_kind",
            Self::OldEpoch(..) => "stale_epoch",
            Self::FutureEpoch(..) => "impossible_future_epoch",
            Self::Envelope(_) | Self::InvalidPayload | Self::DecodeProto(_) | Self::TlsError(_) => {
                "malformed_envelope"
            }
            Self::Storage(_) | Self::Db(_) | Self::Diesel(_) | Self::ClearPendingCommit(_) => {
                "state_storage_failure"
            }
            Self::CommitValidation(_) => "commit_validation_failure",
            Self::OpenMlsProcessMessage(_) | Self::OpenMlsProcessMessageWithAppData(_) => {
                "mls_processing_failure"
            }
            _ => "group_processing_failure",
        }
    }

    /// True only for supported input that cannot apply after its full prefix.
    /// Local storage failures and missing local keys must keep the head pending.
    pub(crate) fn is_safe_rejection(&self) -> bool {
        match self {
            Self::InvalidPayload
            | Self::Envelope(_)
            | Self::OwnMessageWithoutAttempt
            | Self::InvalidSender { .. }
            | Self::DecodeProto(_)
            | Self::TlsError(_)
            | Self::WrongCredentialType(_)
            | Self::UnsupportedMessageType(_)
            | Self::OldEpoch(..)
            | Self::FutureEpoch(..)
            | Self::IntentAlreadyProcessed
            | Self::MessageAlreadyProcessed(_) => true,
            Self::CommitValidation(error) => error.is_safe_rejection(),
            Self::OpenMlsProcessMessage(error) => rejected_mls_input(error),
            Self::OpenMlsProcessMessageWithAppData(error) => match error {
                ProcessMessageWithAppDataError::OpenMls(error) => rejected_mls_input(error),
                ProcessMessageWithAppDataError::AppDataDecode(_) => true,
                ProcessMessageWithAppDataError::ResolveAppDataCommit(
                    openmls::group::ResolveAppDataCommitError::StageCommit(error),
                ) => rejected_commit(error),
                _ => false,
            },
            _ => false,
        }
    }
}

fn rejected_mls_input(error: &ProcessMessageError<SqlKeyStoreError>) -> bool {
    match error {
        ProcessMessageError::ValidationError(ValidationError::LibraryError(_)) => false,
        ProcessMessageError::ValidationError(ValidationError::UnableToDecrypt(error)) => {
            matches!(
                error,
                MessageDecryptionError::AeadError
                    | MessageDecryptionError::GenerationOutOfBound
                    | MessageDecryptionError::MalformedContent
                    | MessageDecryptionError::WrongWireFormat
                    // These secrets were consumed or discarded for forward secrecy.
                    // Local ratchet, index, library, and crypto failures stay blocked.
                    | MessageDecryptionError::SecretTreeError(
                        SecretTreeError::TooDistantInThePast | SecretTreeError::SecretReuseError
                    )
            )
        }
        ProcessMessageError::ValidationError(_)
        | ProcessMessageError::IncompatibleWireFormat
        | ProcessMessageError::UnauthorizedExternalApplicationMessage
        | ProcessMessageError::UnauthorizedExternalCommitMessage
        | ProcessMessageError::UnsupportedProposalType
        | ProcessMessageError::MalformedSafeAad => true,
        ProcessMessageError::InvalidCommit(error) => rejected_commit(error),
        _ => false,
    }
}

fn rejected_commit(error: &StageCommitError) -> bool {
    match error {
        StageCommitError::LibraryError(_)
        | StageCommitError::OwnKeyNotFound
        | StageCommitError::MissingDecryptionKey => false,
        // An absent proposal cannot arrive before a complete ordered prefix.
        StageCommitError::MissingProposal
        | StageCommitError::EpochMismatch
        | StageCommitError::OwnCommitMismatch
        | StageCommitError::WrongPlaintextContentType
        | StageCommitError::PathLeafNodeVerificationFailure
        | StageCommitError::RequiredPathNotFound
        | StageCommitError::ConfirmationTagMissing
        | StageCommitError::ConfirmationTagMismatch
        | StageCommitError::AttemptedSelfRemoval
        | StageCommitError::InconsistentSenderIndex
        | StageCommitError::SenderTypeExternal
        | StageCommitError::SenderTypeNewMemberProposal
        | StageCommitError::TooManyNewMembers
        | StageCommitError::ProposalValidationError(_)
        | StageCommitError::PskError(_)
        | StageCommitError::ExternalCommitValidation(_)
        | StageCommitError::UpdatePathError(_)
        | StageCommitError::VerifiedUpdatePathError(_)
        | StageCommitError::GroupContextExtensionsProposalValidationError(_)
        | StageCommitError::AppDataUpdateValidationError(_)
        | StageCommitError::LeafNodeValidation(_)
        | StageCommitError::ApplyAppDataUpdateError(_)
        | StageCommitError::DuplicatePskId(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[xmtp_common::test(unwrap_try = true)]
    fn obsolete_generations_are_rejected_but_local_secret_tree_failures_stay_pending() {
        for (error, rejected) in [
            (SecretTreeError::SecretReuseError, true),
            (SecretTreeError::TooDistantInThePast, true),
            (SecretTreeError::TooDistantInTheFuture, false),
            (SecretTreeError::IndexOutOfBounds, false),
            (SecretTreeError::RatchetTypeError, false),
            (SecretTreeError::RatchetTooLong, false),
            (SecretTreeError::LibraryError, false),
            (
                SecretTreeError::CryptoError(
                    openmls_traits::types::CryptoError::CryptoLibraryError,
                ),
                false,
            ),
        ] {
            let error = GroupMessageProcessingError::OpenMlsProcessMessage(
                ProcessMessageError::ValidationError(ValidationError::UnableToDecrypt(
                    MessageDecryptionError::SecretTreeError(error),
                )),
            );
            assert_eq!(error.is_safe_rejection(), rejected, "{error:?}");
        }
    }
}
