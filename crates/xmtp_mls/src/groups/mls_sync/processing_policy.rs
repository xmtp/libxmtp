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
            Self::LostProposalState => "lost_proposal_state",
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

    /// True when staging failed because a committed proposal reference is not
    /// in the local proposal store. Only the durable prefix evidence can say
    /// whether the prefix never carried it or local state lost it.
    pub(crate) fn is_missing_proposal(&self) -> bool {
        use ProcessMessageWithAppDataError::{OpenMls, ResolveAppDataCommit};
        use openmls::group::ResolveAppDataCommitError::StageCommit;
        match self {
            Self::OpenMlsProcessMessage(error)
            | Self::OpenMlsProcessMessageWithAppData(OpenMls(error)) => matches!(
                error,
                ProcessMessageError::InvalidCommit(StageCommitError::MissingProposal)
            ),
            Self::OpenMlsProcessMessageWithAppData(ResolveAppDataCommit(StageCommit(error))) => {
                matches!(error, StageCommitError::MissingProposal)
            }
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
        // Terminal only once `is_missing_proposal` has ruled out lost local state.
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

    /// A received commit that Adds a key package whose last-resort mark
    /// carries data is a terminal rejection: the receiver keeps
    /// its epoch, and a later commit that Adds a package with an empty mark
    /// applies. The error is the one libxmtp's receive path gets from
    /// OpenMLS, mapped as that path maps it.
    // verifies: JOIN-082
    #[xmtp_common::test(unwrap_try = true)]
    async fn a_commit_adding_a_last_resort_mark_with_data_is_rejected() {
        use openmls::{
            extensions::{
                AppDataDictionary, AppDataDictionaryExtension, Extension, ExtensionType, Extensions,
            },
            prelude::{
                BasicCredential, Capabilities, CredentialWithKey, KeyPackage, KeyPackageBundle,
                MlsGroup, MlsGroupCreateConfig, MlsGroupJoinConfig, MlsMessageBodyIn, MlsMessageIn,
                MlsMessageOut, OpenMlsProvider, ProcessedMessageContent, StagedWelcome,
                tls_codec::{Deserialize as _, Serialize as _},
            },
        };
        use openmls_basic_credential::SignatureKeyPair;
        use xmtp_cryptography::configuration::CIPHERSUITE;
        use xmtp_db::{
            XmtpTestDb, sql_key_store::SqlKeyStore, xmtp_openmls_provider::XmtpOpenMlsProvider,
        };
        use xmtp_mls_common::libxmtp_version::LibXMTPVersion;

        /// The last-resort mark of the MLS extensions draft.
        const LAST_RESORT_COMPONENT: u16 = 0x0004;

        fn key_package(
            provider: &impl OpenMlsProvider,
            name: &str,
            last_resort_mark: Option<Vec<u8>>,
        ) -> KeyPackageBundle {
            let signer = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm()).unwrap();
            let mut builder = KeyPackage::builder().leaf_node_capabilities(Capabilities::new(
                None,
                Some(&[CIPHERSUITE]),
                Some(&[ExtensionType::AppDataDictionary]),
                None,
                None,
            ));
            if let Some(data) = last_resort_mark {
                let mut dictionary = AppDataDictionary::new();
                let _ = dictionary.insert(LAST_RESORT_COMPONENT, data);
                builder = builder.key_package_extensions(
                    Extensions::from_vec(vec![Extension::AppDataDictionary(
                        AppDataDictionaryExtension::new(dictionary),
                    )])
                    .unwrap(),
                );
            }
            builder
                .build(
                    CIPHERSUITE,
                    provider,
                    &signer,
                    CredentialWithKey {
                        credential: BasicCredential::new(name.as_bytes().to_vec()).into(),
                        signature_key: signer.to_public_vec().into(),
                    },
                )
                .unwrap()
        }
        fn received(message: &MlsMessageOut) -> MlsMessageIn {
            MlsMessageIn::tls_deserialize_exact(message.tls_serialize_detached().unwrap()).unwrap()
        }

        let (alice_store, bob_store) = (
            xmtp_db::TestDb::create_persistent_store(None).await,
            xmtp_db::TestDb::create_persistent_store(None).await,
        );
        let alice = XmtpOpenMlsProvider::new(SqlKeyStore::new(alice_store.conn()));
        let bob = XmtpOpenMlsProvider::new(SqlKeyStore::new(bob_store.conn()));
        let throwaway = openmls_rust_crypto::OpenMlsRustCrypto::default();
        let own = LibXMTPVersion::parse(env!("CARGO_PKG_VERSION"))?;

        let alice_signer = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm())?;
        let mut alice_group = MlsGroup::new(
            &alice,
            &alice_signer,
            &MlsGroupCreateConfig::builder()
                .ciphersuite(CIPHERSUITE)
                .use_ratchet_tree_extension(true)
                .build(),
            CredentialWithKey {
                credential: BasicCredential::new(b"alice".to_vec()).into(),
                signature_key: alice_signer.to_public_vec().into(),
            },
        )?;
        let (_, welcome, _) = alice_group.add_members(
            &alice,
            &alice_signer,
            &[key_package(&bob, "bob", None).key_package().clone()],
        )?;
        alice_group.merge_pending_commit(&alice)?;
        let MlsMessageBodyIn::Welcome(welcome) = received(&welcome).extract() else {
            panic!("not a Welcome");
        };
        let mut bob_group = StagedWelcome::new_from_welcome(
            &bob,
            &MlsGroupJoinConfig::builder()
                .use_ratchet_tree_extension(true)
                .build(),
            welcome,
            None,
        )?
        .into_group(&bob)?;
        let epoch = bob_group.epoch();

        let malformed = key_package(&throwaway, "malformed", Some(vec![1]));
        let (commit, _, _) =
            alice_group.add_members(&alice, &alice_signer, &[malformed.key_package().clone()])?;
        let error = crate::groups::app_data::process_message_with_app_data(
            &mut bob_group,
            &bob,
            received(&commit).try_into_protocol_message()?,
            &own,
        )
        .err()
        .expect("the commit applied");
        let error = GroupMessageProcessingError::from_app_data_processing(error);
        assert!(
            format!("{error:?}").contains("MalformedLastResortComponent"),
            "{error:?}"
        );
        assert!(error.is_safe_rejection(), "{error:?}");
        assert_eq!(bob_group.epoch(), epoch);

        alice_group.clear_pending_commit(alice.storage())?;
        let marked = key_package(&throwaway, "marked", Some(vec![]));
        let (commit, _, _) =
            alice_group.add_members(&alice, &alice_signer, &[marked.key_package().clone()])?;
        alice_group.merge_pending_commit(&alice)?;
        let processed = crate::groups::app_data::process_message_with_app_data(
            &mut bob_group,
            &bob,
            received(&commit).try_into_protocol_message()?,
            &own,
        )?;
        let ProcessedMessageContent::StagedCommitMessage(staged) = processed.into_content() else {
            panic!("not a commit");
        };
        bob_group.merge_staged_commit(&bob, *staged)?;
        assert_eq!(bob_group.epoch().as_u64(), epoch.as_u64() + 1);
        assert_eq!(bob_group.members().count(), 3);
    }
}
