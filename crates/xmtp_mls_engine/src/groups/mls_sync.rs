use super::{
    FailedInstallationIds, GroupError, HmacKey, MlsGroup,
    change_callbacks::AppDataChange,
    intents::{
        CommitPendingProposalsIntentData, Installation, IntentError, PostCommitAction,
        ProposeMemberUpdateIntentData, SendMessageIntentData, SendWelcomesAction,
        UpdateAdminListIntentData, UpdateGroupMembershipIntentData, UpdatePermissionIntentData,
    },
    summary::{MessageIdentifier, MessageIdentifierBuilder, ProcessSummary, SyncSummary},
    validated_commit::{CommitValidationError, validate_proposal},
};
use crate::{
    groups::{
        context::XmtpSharedContext,
        group_membership::GroupMembership,
        hash::sha256,
        intents::{
            MembershipDiffWithKeyPackages, QueueIntent, ReaddInstallationsIntentData,
            UpdateMetadataIntentData,
        },
        message_id::{calculate_message_id, calculate_message_id_for_intent},
        mls_ext::{CommitLogStorer, MlsGroupReload},
        mls_sync::update_group_membership::apply_readd_installations_intent,
        traits::IntoWith,
        validated_commit::ValidatedCommit,
    },
    intents::ProcessIntentError,
};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use openmls::prelude::BasicCredentialError;
use openmls::{
    credentials::BasicCredential,
    framing::ProtocolMessage,
    group::{
        CommitToPendingProposalsError, GroupContext, GroupEpoch, ProcessMessageError, StagedCommit,
        ValidationError,
    },
    key_packages::KeyPackage,
    messages::proposals::Proposal,
    prelude::{
        Extensions, LeafNodeIndex, MlsGroup as OpenMlsGroup, ProcessedMessage,
        ProcessedMessageContent, ProposalType, Sender,
        tls_codec::{Error as TlsCodecError, Serialize},
    },
    treesync::LeafNodeParameters,
};
use openmls_traits::OpenMlsProvider;
use prost::Message;
use prost::bytes::Bytes;
use sha2::Sha256;
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    mem::{Discriminant, discriminant},
    ops::RangeInclusive,
    time::Duration,
};
use thiserror::Error;
use tracing::debug;
use update_group_membership::apply_update_group_membership_intent;
use xmtp_api::PublishUnit;
use xmtp_common::{Event, ExponentialBackoff, RetryableError, Strategy, log_event, time::now_ns};
use xmtp_configuration::{
    HMAC_SALT, MAX_GROUP_SYNC_RETRIES, MAX_PAST_EPOCHS, SYNC_BACKOFF_TOTAL_WAIT_MAX_SECS,
    SYNC_BACKOFF_WAIT_MS, SYNC_JITTER_MS, SYNC_UPDATE_INSTALLATIONS_INTERVAL_NS,
    WELCOME_HPKE_LABEL,
};
use xmtp_content_types::{CodecError, ContentCodec, group_updated::GroupUpdatedCodec};
use xmtp_db::TransactionOutcome::{Continue, Rollback};
use xmtp_db::XmtpMlsStorageProvider;
use xmtp_db::group::GroupMembershipState;
use xmtp_db::{
    Fetch, StorageError, StoreOrIgnore, TransactionOutcome,
    group::ConversationType,
    group_intent::{ID, IntentKind, IntentState, StoredGroupIntent},
    group_message::{ContentType, DeliveryStatus, GroupMessageKind, StoredGroupMessage},
    remote_commit_log::CommitResult,
    sql_key_store,
    user_preferences::ensure_root_key,
};
use xmtp_db::{NotFound, group_intent::IntentKind::MetadataUpdate};
use xmtp_db::{XmtpOpenMlsProviderRef, prelude::*};
use xmtp_db::{
    group_message::MsgQueryArgs,
    pending_remove::{PendingRemove, QueryPendingRemove},
};
use xmtp_events::{ClientEvent, EventContext, EventWriter};
use xmtp_id::{InboxId, InboxIdRef};
use xmtp_mls_core::{
    hmac_epoch::hmac_epoch,
    identity::{IdentityError, parse_credential},
    mls_store::MlsStore,
};
use xmtp_mls_group_keys::mls_ext::payload_encryption::{
    WrapPayloadError, wrap_payload_hpke, wrap_payload_symmetric,
};
use xmtp_mls_group_state::app_data::component_id::ComponentId;
use xmtp_mls_group_state::group_mutable_metadata::MetadataField;
use xmtp_mls_group_state::libxmtp_version::LibXMTPVersion;
use xmtp_mls_identity::identity_updates::{
    IdentityUpdateError, IdentityUpdates, load_identity_updates_for_client,
};
use xmtp_mls_messages::{decoded_message::MessageBody, enrichment::EnrichMessageError};
use xmtp_mls_validation::commit::{
    CommitRuleError, Inbox, MetadataChanges, extract_group_membership,
};
use xmtp_proto::ShortHex;
use xmtp_proto::backend_v1::{
    ClientEnvelope, GroupMessage as BackendGroupMessage, WelcomeMessage as WelcomeMessageInput,
    client_envelope::Payload,
    welcome_message::{
        V1 as WelcomeMessageInputV1, Version as WelcomeMessageInputVersion,
        WelcomePointer as WelcomePointerInput,
    },
};
use xmtp_proto::types::GroupId;
use xmtp_proto::xmtp::mls::{
    database::{ProcessPendingSelfRemove, Task as TaskProto, task::Task as TaskKind},
    message_contents::{
        GroupUpdated, PlaintextEnvelope, WelcomeMetadata, WelcomePointer as WelcomePointerProto,
        group_updated,
        plaintext_envelope::{Content, V2},
    },
};
use xmtp_proto::{
    GroupUpdateDeduper,
    types::{Cursor, GroupMessage},
};
use zeroize::Zeroizing;

mod envelope_and_app_data;
mod external_message;
mod leave_and_delete;
mod membership_helpers;
mod own_message;
mod post_commit;
mod processing;
mod receive;
#[cfg(any(test, feature = "test-utils"))]
pub(crate) use receive::PAGE_SIZE as RECEIVE_PAGE_SIZE;
mod sync_driver;
#[cfg(any(test, feature = "test-hooks"))]
pub mod test_mocks;
pub(in crate::groups) use membership_helpers::*;
mod processing_policy;
pub mod update_group_membership;
pub use processing::GroupHeadOutcome;
pub(crate) mod publish;

impl<Context: XmtpSharedContext> MlsGroup<Context> {
    fn metadata_changed_event(&self, commit: &ValidatedCommit) -> Option<ClientEvent> {
        let changed: BTreeSet<_> = commit
            .metadata_component_ids
            .iter()
            .copied()
            .map(ComponentId::from)
            .map(ComponentId::event_name)
            .collect();
        (!changed.is_empty()).then(|| {
            ClientEvent::ConversationMetadataChanged(xmtp_events::MetadataChanged {
                group_id: self.group_id.to_vec(),
                changed: changed.into_iter().collect(),
            })
        })
    }

    pub(in crate::groups) fn emit_message_status_changed(
        &self,
        message_id: Vec<u8>,
        previous: xmtp_events::MessageStatus,
        current: xmtp_events::MessageStatus,
        writer: &impl EventWriter<xmtp_mls_core::events::InternalEvent>,
    ) {
        if self.conversation_type.is_virtual() || previous == current {
            return;
        }
        writer.emit_with_context(
            Some(ClientEvent::MessageStatusChanged(
                xmtp_events::MessageStatusChanged {
                    group_id: self.group_id.to_vec(),
                    message_id,
                    previous,
                    current,
                },
            )),
            None,
            EventContext {
                dm_identifier: self.dm_id.as_ref().map(|id| id.as_bytes().to_vec()),
                references_own_messages: false,
                ..Default::default()
            },
        );
    }

    /// Queue the public changes of one applied commit in kind-table order.
    fn emit_commit_events(
        &self,
        commit: &ValidatedCommit,
        group_active: bool,
        storage: &impl XmtpMlsStorageProvider,
        writer: &impl EventWriter<xmtp_mls_core::events::InternalEvent>,
    ) -> Result<(), GroupMessageProcessingError> {
        if self.conversation_type.is_virtual() {
            return Ok(());
        }
        let context = EventContext {
            dm_identifier: self.dm_id.as_ref().map(|id| id.as_bytes().to_vec()),
            references_own_messages: false,
            ..Default::default()
        };
        if !group_active {
            let left = storage
                .db()
                .get_pending_remove_users(&self.group_id)?
                .contains(&self.context.inbox_id().to_string());
            writer.emit_with_context(
                Some(ClientEvent::ConversationRemoved(
                    xmtp_events::ConversationRemoved {
                        group_id: self.group_id.to_vec(),
                        cause: if left {
                            xmtp_events::RemovalCause::Left
                        } else {
                            xmtp_events::RemovalCause::Removed
                        },
                    },
                )),
                None,
                context.clone(),
            );
        }
        if !commit.added_inboxes.is_empty() || !commit.removed_inboxes.is_empty() {
            writer.emit_with_context(
                Some(ClientEvent::ConversationMembershipChanged(
                    xmtp_events::MembershipChanged {
                        group_id: self.group_id.to_vec(),
                        added_inbox_ids: commit
                            .added_inboxes
                            .iter()
                            .map(|inbox| inbox.inbox_id.clone())
                            .collect(),
                        removed_inbox_ids: commit
                            .removed_inboxes
                            .iter()
                            .map(|inbox| inbox.inbox_id.clone())
                            .collect(),
                    },
                )),
                None,
                context.clone(),
            );
        }
        if let Some(event) = self.metadata_changed_event(commit) {
            writer.emit_with_context(Some(event), None, context);
        }
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum GroupMessageProcessingError {
    #[error("intent already processed")]
    IntentAlreadyProcessed,
    #[error("message with cursor [{}] for group [{}] already processed", _0.cursor, xmtp_common::fmt::debug_hex(_0.group_id)
    )]
    MessageAlreadyProcessed(Box<MessageIdentifier>),
    #[error("message identifier not found")]
    MessageIdentifierNotFound,
    #[error("welcome with cursor [{0}] already processed")]
    WelcomeAlreadyProcessed(u64),
    #[error("[{message_time_ns:?}] invalid sender with credential: {credential:?}")]
    InvalidSender {
        message_time_ns: u64,
        credential: Vec<u8>,
    },
    #[error("invalid payload")]
    InvalidPayload,
    /// The received prefix changed before this attempt acquired the writer.
    #[error("incoming group head changed")]
    IncomingHeadChanged,
    /// The local pending record cannot be decoded. Processing must stop.
    #[error("stored incoming envelope is corrupt: {0}")]
    CorruptIncomingEnvelope(prost::DecodeError),
    /// The MLS wire version is not supported. Keep this envelope pending.
    #[error("unsupported MLS wire version")]
    UnsupportedMlsVersion,
    /// A commit names a proposal the ordered prefix delivered, but the local
    /// proposal store no longer holds it. This is lost local state, not
    /// evidence that the commit is invalid, so the head is held.
    #[error("a committed proposal was received but is missing from local state")]
    LostProposalState,
    /// An application sender is beyond the fixed forward window. Not retryable.
    #[error("application sender generation exceeds the forward window")]
    FarForwardApplicationMessage,
    /// An application sender names a leaf outside the ratchet tree. Not retryable.
    #[error("application sender is outside the ratchet tree")]
    ApplicationSenderOutsideTree,
    /// Supported envelope framing fails validation after its complete prefix.
    #[error(transparent)]
    Envelope(xmtp_api_backend::envelope::EnvelopeError),
    /// Own ciphertext has no durable prepared attempt.
    #[error("own envelope has no prepared attempt")]
    OwnMessageWithoutAttempt,
    /// Our own echo names an intent whose kind a newer build wrote and this
    /// one cannot decode.
    ///
    /// This holds the head rather than rejecting it. The envelope may be a
    /// commit every other member applied; skipping it would leave this
    /// installation behind the group with no way back. It is also not an
    /// external message — treating it as one would validate a commit we
    /// authored against external-actor rules. Not retryable, so the head is
    /// marked blocked and reconsidered once a build that can read the kind
    /// runs again.
    #[error("own intent kind for payload {0} is not supported by this version")]
    UnsupportedOwnIntentKind(String),
    /// A local prepared attempt cannot safely explain this own envelope.
    #[error("prepared attempt state: {0}")]
    PreparedAttempt(#[source] Box<GroupError>),
    /// A stored terminal rejection whose original parameters were not retained.
    #[error("intent rejected: {0}")]
    RejectedIntent(&'static str),
    #[error("storage error: {0}")]
    Storage(#[from] xmtp_db::StorageError),
    #[error(transparent)]
    Identity(#[from] IdentityError),
    #[error("openmls process message error: {0}")]
    OpenMlsProcessMessage(
        #[from] openmls::prelude::ProcessMessageError<sql_key_store::SqlKeyStoreError>,
    ),
    /// AppDataUpdate-aware processing wrapper error.
    ///
    /// Wraps the same `ProcessMessageError` as the variant above, plus the
    /// `ComponentSourceError` that fires when an incoming `AppDataUpdate`
    /// payload can't be decoded under our wire format. Kept distinct from
    /// `OpenMlsProcessMessage` so the AppData-decode failure mode is
    /// greppable in logs.
    #[error("app-data process message error: {0}")]
    OpenMlsProcessMessageWithAppData(
        #[from] super::app_data::ProcessMessageWithAppDataError<sql_key_store::SqlKeyStoreError>,
    ),
    #[error("merge staged commit: {0}")]
    MergeStagedCommit(#[from] openmls::group::MergeCommitError<sql_key_store::SqlKeyStoreError>),
    #[error("TLS Codec error: {0}")]
    TlsError(#[from] TlsCodecError),
    #[error("unsupported message type: {0:?}")]
    UnsupportedMessageType(Discriminant<ProtocolMessage>),
    /// Processed-message content that cannot legitimately reach the apply
    /// phase: `UnresolvedAppDataCommit` is always resolved inside
    /// `process_message_with_app_data`, and `OwnPendingCommit` is only
    /// produced for public-framed commits, which libxmtp's
    /// pure-ciphertext wire format policy never emits.
    #[error("unexpected processed message content: {0}")]
    UnexpectedProcessedContent(&'static str),
    #[error("commit validation: {0}")]
    CommitValidation(#[from] CommitValidationError),
    #[error("epoch increment not allowed")]
    EpochIncrementNotAllowed,
    #[error("clear pending commit error: {0}")]
    ClearPendingCommit(#[from] sql_key_store::SqlKeyStoreError),
    #[error("Serialization/Deserialization Error {0}")]
    Serde(#[from] serde_json::Error),
    #[error("intent is missing staged_commit field")]
    IntentMissingStagedCommit,
    #[error("encode proto: {0}")]
    EncodeProto(#[from] prost::EncodeError),
    #[error("proto decode error: {0}")]
    DecodeProto(#[from] prost::DecodeError),
    #[error(transparent)]
    Intent(#[from] IntentError),
    #[error(transparent)]
    Codec(#[from] CodecError),
    #[error("wrong credential type")]
    WrongCredentialType(#[from] BasicCredentialError),
    #[error(transparent)]
    ProcessIntent(#[from] ProcessIntentError),
    #[error(transparent)]
    AssociationDeserialization(#[from] xmtp_id::associations::DeserializationError),
    #[error(transparent)]
    IdentityUpdate(#[from] IdentityUpdateError),
    #[error("Group paused due to minimum protocol version requirement")]
    GroupPaused,
    /// Processing stops at the removal commit until a valid rejoin Welcome.
    #[error("group is inactive")]
    GroupInactive,
    #[error("Message epoch [{0}] is too old [{1}]")]
    OldEpoch(u64, u64),
    #[error("Message epoch [{0}] is greater than group epoch [{1}]")]
    FutureEpoch(u64, u64),
    #[error(transparent)]
    Db(#[from] xmtp_db::ConnectionError),
    #[error(transparent)]
    Builder(#[from] derive_builder::UninitializedFieldError),
    #[error(transparent)]
    Diesel(#[from] xmtp_db::diesel::result::Error),
    #[error(transparent)]
    EnrichMessage(#[from] EnrichMessageError),
    #[error("pre-commit proposal phase complete, re-queuing intent")]
    PreCommitProposalPhaseComplete,
    #[error(transparent)]
    Conversion(#[from] xmtp_proto::ConversionError),
    /// A successful staged-commit merge by a member that remains in the group
    /// must advance the epoch and therefore change the epoch authenticator.
    /// If it did not, the group state the commit was merged onto was
    /// corrupt/torn (e.g. produced by a cross-process race on a shared MLS
    /// DB). Retryable: the enclosing transaction (cursor + merge + commit
    /// log) rolls back and the message is reprocessed against settled state.
    #[error(
        "staged commit merge for group [{group_id}] at sequence [{commit_sequence_id}] reached \
         epoch [{epoch}] without advancing the epoch authenticator; refusing to record corrupt \
         commit log entry"
    )]
    EpochAuthenticatorNotAdvanced {
        group_id: GroupId,
        commit_sequence_id: i64,
        epoch: u64,
    },
}

impl From<xmtp_mls_validation::commit::CommitRuleError> for GroupMessageProcessingError {
    fn from(error: xmtp_mls_validation::commit::CommitRuleError) -> Self {
        Self::CommitValidation(error.into())
    }
}

impl RetryableError for GroupMessageProcessingError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::IncomingHeadChanged => true,
            Self::PreparedAttempt(error) => error.is_retryable(),
            Self::CorruptIncomingEnvelope(_)
            | Self::UnsupportedMlsVersion
            | Self::LostProposalState
            | Self::FarForwardApplicationMessage
            | Self::ApplicationSenderOutsideTree
            | Self::Envelope(_)
            | Self::OwnMessageWithoutAttempt
            | Self::UnsupportedOwnIntentKind(_) => false,
            Self::RejectedIntent(_) => false,
            Self::Storage(err) => err.is_retryable(),
            Self::Diesel(err) => err.is_retryable(),
            Self::Identity(err) => err.is_retryable(),
            Self::OpenMlsProcessMessage(err) => err.is_retryable(),
            Self::OpenMlsProcessMessageWithAppData(err) => match err {
                super::app_data::ProcessMessageWithAppDataError::OpenMls(e) => e.is_retryable(),
                // Decode failures are wire-format violations from the
                // peer — retrying won't help.
                super::app_data::ProcessMessageWithAppDataError::AppDataDecode(_) => false,
                // Resolved by upgrading, not by retrying. In practice
                // this variant never reaches here: the call sites remap
                // it to `CommitValidation(ProtocolVersionTooLow)` so
                // the pause machinery in `post_process_message` fires.
                super::app_data::ProcessMessageWithAppDataError::ProtocolVersionTooLow {
                    ..
                } => false,
                // Staging failures are validation-shaped — the same
                // `StageCommitError` on a commit without app-data
                // proposals surfaces through the `OpenMls` arm as a
                // non-retryable `InvalidCommit`.
                super::app_data::ProcessMessageWithAppDataError::ResolveAppDataCommit(_) => false,
            },
            Self::MergeStagedCommit(err) => err.is_retryable(),
            Self::ProcessIntent(err) => err.is_retryable(),
            Self::CommitValidation(err) => err.is_retryable(),
            Self::ClearPendingCommit(err) => err.is_retryable(),
            Self::IdentityUpdate(err) => err.is_retryable(),
            Self::Db(e) => e.is_retryable(),
            Self::EnrichMessage(e) => e.is_retryable(),
            Self::IntentAlreadyProcessed
            | Self::MessageIdentifierNotFound
            | Self::WrongCredentialType(_)
            | Self::Codec(_)
            | Self::MessageAlreadyProcessed(_)
            | Self::WelcomeAlreadyProcessed(_)
            | Self::InvalidSender { .. }
            | Self::DecodeProto(_)
            | Self::InvalidPayload
            | Self::Intent(_)
            | Self::UnexpectedProcessedContent(_)
            | Self::EpochIncrementNotAllowed
            | Self::EncodeProto(_)
            | Self::IntentMissingStagedCommit
            | Self::Serde(_)
            | Self::AssociationDeserialization(_)
            | Self::TlsError(_)
            | Self::UnsupportedMessageType(_)
            | Self::GroupPaused
            | Self::GroupInactive
            | Self::FutureEpoch(_, _)
            | Self::OldEpoch(_, _)
            | Self::PreCommitProposalPhaseComplete => false,
            Self::Builder(_) => false,
            Self::Conversion(_) => false,
            // Retry so the enclosing transaction rolls back (including the
            // cursor advance) and the message converges via cursor dedup
            // instead of persisting a forked commit log entry.
            Self::EpochAuthenticatorNotAdvanced { .. } => true,
        }
    }

    fn rate_limit_delay(&self) -> Option<xmtp_common::time::Duration> {
        match self {
            Self::PreparedAttempt(error) => error.rate_limit_delay(),
            Self::Identity(err) => err.rate_limit_delay(),
            Self::IdentityUpdate(err) => err.rate_limit_delay(),
            Self::CommitValidation(err) => err.rate_limit_delay(),
            _ => None,
        }
    }
}

impl xmtp_mls_core::worker::NeedsDbReconnect for GroupMessageProcessingError {
    #[deny(clippy::wildcard_enum_match_arm)]
    fn needs_db_reconnect(&self) -> bool {
        match self {
            Self::Storage(error) => error.db_needs_connection(),
            Self::Db(error) => error.db_needs_connection(),
            Self::IdentityUpdate(error) => error.needs_db_reconnect(),
            Self::Identity(error) => error.needs_db_reconnect(),
            Self::PreparedAttempt(error) => error.needs_db_reconnect(),
            Self::CommitValidation(error) => error.needs_db_reconnect(),
            Self::IntentAlreadyProcessed
            | Self::MessageAlreadyProcessed(..)
            | Self::MessageIdentifierNotFound
            | Self::WelcomeAlreadyProcessed(..)
            | Self::InvalidSender { .. }
            | Self::InvalidPayload
            | Self::IncomingHeadChanged
            | Self::CorruptIncomingEnvelope(..)
            | Self::UnsupportedMlsVersion
            | Self::LostProposalState
            | Self::FarForwardApplicationMessage
            | Self::ApplicationSenderOutsideTree
            | Self::Envelope(..)
            | Self::OwnMessageWithoutAttempt
            | Self::UnsupportedOwnIntentKind(..)
            | Self::RejectedIntent(..)
            | Self::OpenMlsProcessMessage(..)
            | Self::OpenMlsProcessMessageWithAppData(..)
            | Self::MergeStagedCommit(..)
            | Self::TlsError(..)
            | Self::UnsupportedMessageType(..)
            | Self::UnexpectedProcessedContent(..)
            | Self::EpochIncrementNotAllowed
            | Self::ClearPendingCommit(..)
            | Self::Serde(..)
            | Self::IntentMissingStagedCommit
            | Self::EncodeProto(..)
            | Self::DecodeProto(..)
            | Self::Intent(..)
            | Self::Codec(..)
            | Self::WrongCredentialType(..)
            | Self::ProcessIntent(..)
            | Self::AssociationDeserialization(..)
            | Self::GroupPaused
            | Self::GroupInactive
            | Self::OldEpoch(..)
            | Self::FutureEpoch(..)
            | Self::Builder(..)
            | Self::Diesel(..)
            | Self::EnrichMessage(..)
            | Self::PreCommitProposalPhaseComplete
            | Self::Conversion(..)
            | Self::EpochAuthenticatorNotAdvanced { .. } => false,
        }
    }
}

impl GroupMessageProcessingError {
    /// Route an app-data processing failure into this error type.
    ///
    /// The pre-dispatch floor guard in `process_message_with_app_data`
    /// surfaces as the same `ProtocolVersionTooLow` commit-validation
    /// error the validator's post-policy floor check emits, so
    /// `post_process_message` pauses the group (held cursor,
    /// `set_group_paused`) instead of treating it like a wire-format
    /// rejection that advances past the commit. Every other variant
    /// wraps as `OpenMlsProcessMessageWithAppData`, same as `From`.
    /// Use this instead of `?`'s implicit conversion at call sites
    /// that can see commits from newer protocol versions.
    fn from_app_data_processing(
        err: super::app_data::ProcessMessageWithAppDataError<sql_key_store::SqlKeyStoreError>,
    ) -> Self {
        match err {
            super::app_data::ProcessMessageWithAppDataError::ProtocolVersionTooLow {
                min_version,
                ..
            } => Self::CommitValidation(CommitValidationError::Rule(
                CommitRuleError::ProtocolVersionTooLow(min_version),
            )),
            other => other.into(),
        }
    }

    pub(crate) fn commit_result(&self) -> CommitResult {
        use super::app_data::ProcessMessageWithAppDataError;
        match self {
            GroupMessageProcessingError::OpenMlsProcessMessage(
                ProcessMessageError::ValidationError(ValidationError::WrongEpoch),
            ) => CommitResult::WrongEpoch,
            // Treat the AppData-aware wrapper the same as the bare
            // OpenMLS error: if it carries a ValidationError(WrongEpoch),
            // surface as WrongEpoch; if it carries any other OpenMLS
            // error, surface as Undecryptable. Decode failures (the
            // AppData-side variant) are treated as Invalid because they
            // mean the peer's wire format was wrong.
            GroupMessageProcessingError::OpenMlsProcessMessageWithAppData(
                ProcessMessageWithAppDataError::OpenMls(ProcessMessageError::ValidationError(
                    ValidationError::WrongEpoch,
                )),
            ) => CommitResult::WrongEpoch,
            GroupMessageProcessingError::OpenMlsProcessMessageWithAppData(
                ProcessMessageWithAppDataError::OpenMls(_),
            ) => CommitResult::Undecryptable,
            GroupMessageProcessingError::OpenMlsProcessMessageWithAppData(
                ProcessMessageWithAppDataError::AppDataDecode(_),
            ) => CommitResult::Invalid,
            GroupMessageProcessingError::OldEpoch(_, _) => CommitResult::WrongEpoch,
            GroupMessageProcessingError::FutureEpoch(_, _) => CommitResult::WrongEpoch,
            GroupMessageProcessingError::CommitValidation(_) => CommitResult::Invalid,
            GroupMessageProcessingError::OpenMlsProcessMessage(_) => CommitResult::Undecryptable,
            _ => CommitResult::Unknown,
        }
    }
}

#[derive(Debug, Error)]
pub struct IntentResolutionError {
    processing_error: GroupMessageProcessingError,
}

impl std::fmt::Display for IntentResolutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "IntentValidationError: {}", self.processing_error)
    }
}

impl RetryableError for IntentResolutionError {
    fn is_retryable(&self) -> bool {
        self.processing_error.is_retryable()
    }

    fn rate_limit_delay(&self) -> Option<xmtp_common::time::Duration> {
        self.processing_error.rate_limit_delay()
    }
}

#[derive(Debug)]
pub(crate) struct PublishIntentData {
    pub(crate) staged_commit: Option<Vec<u8>>,
    pub(crate) post_commit_action: Option<Vec<u8>>,
    /// One or more payloads to publish. Most intents have a single payload (commit or message),
    /// but proposal intents may have multiple payloads (one per proposal).
    pub(crate) payloads_to_publish: Vec<Vec<u8>>,
    pub(crate) should_send_push_notification: bool,
    pub(crate) group_epoch: u64,
}

#[cfg(any(test, feature = "test-utils"))]
impl PublishIntentData {
    #[allow(dead_code)]
    pub fn post_commit_data(&self) -> Option<Vec<u8>> {
        self.post_commit_action.clone()
    }

    #[allow(dead_code)]
    pub fn staged_commit(&self) -> Option<Vec<u8>> {
        self.staged_commit.clone()
    }
}

/// Post-commit work from one processed durable group head.
#[derive(Debug)]
pub struct ProcessedMessageOutcome {
    /// Active state from the same transaction that processed the message.
    /// The controller retires the group after that transaction commits.
    pub(crate) group_active: bool,
    /// True when this message stored a disappearing (expiring) message. The
    /// disappearing worker is re-armed *after* the storage transaction commits
    /// (see `process_message`), so the worker's `min_expire_at_ns`
    /// query is guaranteed to observe the newly written `expire_at_ns`.
    /// Set when processing this message changed the group's `app_data`.
    /// Carried out of the state writer so the host callback can be
    /// awaited after commit; `None` whenever no
    /// callback is registered, since the snapshot is skipped entirely then.
    pub(crate) app_data_change: Option<AppDataChange>,
}

impl ProcessedMessageOutcome {
    /// An outcome with no worker wake or app-data callback.
    fn new(group_active: bool) -> Self {
        Self {
            group_active,
            app_data_change: None,
        }
    }
}

#[cfg(test)]
pub(crate) mod tests;
