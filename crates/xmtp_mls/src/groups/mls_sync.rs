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
    client::ClientError,
    context::XmtpSharedContext,
    groups::{
        group_membership::GroupMembership,
        intents::MembershipDiffWithKeyPackages,
        intents::{QueueIntent, ReaddInstallationsIntentData, UpdateMetadataIntentData},
        mls_ext::{CommitLogStorer, MlsGroupReload},
        mls_sync::update_group_membership::apply_readd_installations_intent,
        validated_commit::ValidatedCommit,
    },
    identity::{IdentityError, parse_credential},
    identity_updates::{IdentityUpdates, load_identity_updates},
    intents::ProcessIntentError,
    messages::{decoded_message::MessageBody, enrichment::EnrichMessageError},
    mls_store::MlsStore,
    subscriptions::SyncWorkerEvent,
    traits::IntoWith,
    utils::{
        hash::sha256,
        id::{calculate_message_id, calculate_message_id_for_intent},
        time::hmac_epoch,
    },
    worker::WorkerKind,
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
    collections::{HashMap, HashSet, VecDeque},
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
use xmtp_db::message_deletion::{QueryMessageDeletion, StoredMessageDeletion};
use xmtp_db::{
    Fetch, StorageError, StoreOrIgnore, TransactionOutcome,
    group::{ConversationType, StoredGroup},
    group_intent::{ID, IntentKind, IntentState, StoredGroupIntent},
    group_message::{ContentType, DeliveryStatus, GroupMessageKind, StoredGroupMessage},
    remote_commit_log::CommitResult,
    sql_key_store,
    user_preferences::StoredUserPreferences,
};
use xmtp_db::{NotFound, group_intent::IntentKind::MetadataUpdate};
use xmtp_db::{XmtpOpenMlsProviderRef, prelude::*};
use xmtp_db::{group::GroupMembershipState, group_message::Deletable};
use xmtp_db::{
    group_message::MsgQueryArgs,
    pending_remove::{PendingRemove, QueryPendingRemove},
};
use xmtp_id::{InboxId, InboxIdRef};
use xmtp_mls_common::group_mutable_metadata::MetadataField;
use xmtp_mls_common::libxmtp_version::LibXMTPVersion;
use xmtp_mls_common::mls_ext::payload_encryption::{
    WrapPayloadError, wrap_payload_hpke, wrap_payload_symmetric,
};
use xmtp_mls_validation::commit::{
    CommitRuleError, Inbox, MetadataChanges, extract_group_membership,
};
use xmtp_proto::backend_v1::{
    ClientEnvelope, GroupMessage as BackendGroupMessage, WelcomeMessage as WelcomeMessageInput,
    client_envelope::Payload,
    welcome_message::{
        V1 as WelcomeMessageInputV1, Version as WelcomeMessageInputVersion,
        WelcomePointer as WelcomePointerInput,
    },
};
use xmtp_proto::types::GroupId;
use xmtp_proto::xmtp::mls::message_contents::content_types::DeleteMessage;
use xmtp_proto::xmtp::mls::{
    database::{ProcessPendingSelfRemove, Task as TaskProto, task::Task as TaskKind},
    message_contents::{
        GroupUpdated, PlaintextEnvelope, WelcomeMetadata, WelcomePointer as WelcomePointerProto,
        group_updated,
        plaintext_envelope::{Content, V1, V2},
    },
};
use xmtp_proto::{
    GroupUpdateDeduper,
    types::{Cursor, GroupMessage},
};
use xmtp_proto::{ShortHex, xmtp::mls::message_contents::EncodedContent};
use zeroize::Zeroizing;

mod envelope_and_app_data;
mod external_message;
mod leave_and_delete;
mod membership_helpers;
mod own_message;
mod post_commit;
mod processing;
mod receive;
mod sync_driver;
pub(in crate::groups) use membership_helpers::*;
mod processing_policy;
pub mod update_group_membership;
pub(crate) use processing::GroupHeadOutcome;
pub(crate) mod publish;

#[derive(Debug, Error)]
pub enum GroupMessageProcessingError {
    #[error("intent already processed")]
    IntentAlreadyProcessed,
    #[error("message with cursor [{}] for group [{}] already processed", _0.cursor, xmtp_common::fmt::debug_hex(_0.group_id)
    )]
    MessageAlreadyProcessed(MessageIdentifier),
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
    PreparedAttempt(Box<GroupError>),
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
    Client(#[from] ClientError),
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
            Self::Client(err) => err.is_retryable(),
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
}

impl crate::worker::NeedsDbReconnect for GroupMessageProcessingError {
    fn needs_db_reconnect(&self) -> bool {
        match self {
            Self::Storage(error) => error.db_needs_connection(),
            Self::Db(error) => error.db_needs_connection(),
            Self::Client(error) => error.db_needs_connection(),
            Self::Identity(error) => error.needs_db_reconnect(),
            Self::PreparedAttempt(error) => error.needs_db_reconnect(),
            Self::CommitValidation(error) => error.needs_db_reconnect(),
            _ => false,
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
pub(crate) struct ProcessedMessageOutcome {
    /// Active state from the same transaction that processed the message.
    /// The controller retires the group after that transaction commits.
    pub(crate) group_active: bool,
    /// True when this message stored a disappearing (expiring) message. The
    /// disappearing worker is re-armed *after* the storage transaction commits
    /// (see `process_message`), so the worker's `min_expire_at_ns`
    /// query is guaranteed to observe the newly written `expire_at_ns`.
    pub(crate) disappearing_message_stored: bool,
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
            disappearing_message_stored: false,
            app_data_change: None,
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {

    use super::*;
    use crate::{builder::ClientBuilder, utils::TestMlsGroup};
    use std::sync::Arc;
    use xmtp_cryptography::utils::generate_local_wallet;

    #[xmtp_common::test(unwrap_try = true)]
    async fn publish_stores_envelope_metadata_without_sync() {
        use crate::tester;
        use xmtp_proto::types::Topic;

        tester!(alix, disable_workers);
        // Rotate the group key before the message exists.
        let group = alix.create_group(None, None)?;
        group.key_update().await?;
        let id = group.send_message_optimistic(b"publish metadata", Default::default())?;
        group.publish_intents().await?;
        let published = alix.context.db().find_group_intents(
            group.group_id,
            Some(vec![IntentState::Published]),
            Some(vec![IntentKind::SendMessage]),
        )?;
        assert_eq!(published.len(), 1);

        // Read the local row before any query. Publishing must fill both fields.
        let stored: StoredGroupMessage = alix.context.db().fetch(&id)?.unwrap();
        assert_eq!(stored.sequence_id, 0);
        assert!(
            stored.envelope_hash.is_some(),
            "publish left envelope_hash NULL"
        );
        assert!(stored.expiry_ns.is_some(), "publish left expiry_ns NULL");

        // Read the wire envelope without processing it into the local database.
        let envelopes = alix
            .context
            .api()
            .query_all(
                [(Topic::new_group_message(group.group_id), Cursor(0))].into(),
                alix.context.api().limits().max_query_limit as u32,
            )
            .await?;
        let envelope = envelopes.last().unwrap();
        let canonical = xmtp_mls_validation::parse_envelope(envelope.envelope.clone().unwrap())?;
        assert_eq!(
            stored.envelope_hash,
            Some(canonical.canonical.hash.to_vec())
        );
        assert_eq!(
            stored.expiry_ns,
            Some(i64::try_from(envelope.meta.as_ref().unwrap().expiry_ns)?)
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn partial_envelope_metadata_preserves_stored_fields() {
        use crate::tester;
        use xmtp_proto::types::Topic;

        tester!(alix, disable_workers);
        let group = alix.create_group(None, None)?;
        let id = group
            .send_message(b"partial metadata", Default::default())
            .await?;
        let original: StoredGroupMessage = alix.context.db().fetch(&id)?.unwrap();
        assert!(original.envelope_hash.is_some());
        assert!(original.expiry_ns.is_some());
        let mut envelopes = alix
            .context
            .api()
            .query_all(
                [(Topic::new_group_message(group.group_id), Cursor(0))].into(),
                alix.context.api().limits().max_query_limit as u32,
            )
            .await?;
        let mut envelope =
            xmtp_api_backend::envelope::decode_group_message(envelopes.pop().unwrap())?;
        envelope.envelope_hash = None;
        TestMlsGroup::save_envelope_metadata_with_db(&alix.context.db(), &envelope)?;
        let stored: StoredGroupMessage = alix.context.db().fetch(&id)?.unwrap();
        assert_eq!(stored.envelope_hash, original.envelope_hash);
        assert_eq!(stored.expiry_ns, original.expiry_ns);

        envelope.envelope_hash = original.envelope_hash.clone();
        envelope.expiry_ns = None;
        TestMlsGroup::save_envelope_metadata_with_db(&alix.context.db(), &envelope)?;
        let stored: StoredGroupMessage = alix.context.db().fetch(&id)?.unwrap();
        assert_eq!(stored.envelope_hash, original.envelope_hash);
        assert_eq!(stored.expiry_ns, original.expiry_ns);
    }

    /// This test is not reproducible in webassembly, b/c webassembly has only one thread.
    #[cfg_attr(
        not(target_arch = "wasm32"),
        tokio::test(flavor = "multi_thread", worker_threads = 10)
    )]
    #[cfg(not(target_family = "wasm"))]
    async fn publish_intents_worst_case_scenario() {
        use crate::tester;

        tester!(amal_a, triggers);
        let amal_group_a: Arc<MlsGroup<_>> =
            Arc::new(amal_a.create_group(None, Default::default()).unwrap());

        let db = amal_a.context.db();

        // create group intent
        amal_group_a.sync().await.unwrap();
        assert_eq!(db.intents_processed(), 1);

        for _ in 0..100 {
            use crate::groups::send_message_opts::SendMessageOpts;

            let s = xmtp_common::rand_string::<100>();
            amal_group_a
                .send_message_optimistic(s.as_bytes(), SendMessageOpts::default())
                .unwrap();
        }

        let mut set = tokio::task::JoinSet::new();
        for _ in 0..50 {
            let g = amal_group_a.clone();
            set.spawn(async move { g.publish_intents().await });
        }

        let res = set.join_all().await;
        let errs: Vec<&Result<_, _>> = res.iter().filter(|r| r.is_err()).collect();
        errs.iter().for_each(|e| {
            tracing::error!("{}", e.as_ref().unwrap_err());
        });

        let published = db.intents_published();
        assert_eq!(published, 101);
        let created = db.intents_created();
        assert_eq!(created, 101);
        if !errs.is_empty() {
            panic!("Errors during publish");
        }
    }

    #[xmtp_common::test]
    async fn hmac_keys_work_as_expected() {
        let wallet = generate_local_wallet();
        let amal = Arc::new(ClientBuilder::new_test_client(&wallet).await);
        let amal_group: Arc<TestMlsGroup> =
            Arc::new(amal.create_group(None, Default::default()).unwrap());

        let hmac_keys = amal_group.hmac_keys(-1..=1).unwrap();
        let current_hmac_key = amal_group.hmac_keys(0..=0).unwrap().pop().unwrap();
        assert_eq!(hmac_keys.len(), 3);
        assert_eq!(hmac_keys[1].key, current_hmac_key.key);
        assert_eq!(hmac_keys[1].epoch, current_hmac_key.epoch);

        // Make sure the keys are different
        assert_ne!(hmac_keys[0].key, hmac_keys[1].key);
        assert_ne!(hmac_keys[0].key, hmac_keys[2].key);
        assert_ne!(hmac_keys[1].key, hmac_keys[2].key);

        // Make sure the epochs align
        let current_epoch = hmac_epoch();
        assert_eq!(hmac_keys[0].epoch, current_epoch - 1);
        assert_eq!(hmac_keys[1].epoch, current_epoch);
        assert_eq!(hmac_keys[2].epoch, current_epoch + 1);
    }

    /// Test that process_delete_message handles completely malformed bytes gracefully
    ///
    /// This verifies sync resilience when receiving corrupted DeleteMessage protos.
    #[xmtp_common::test(unwrap_try = true)]
    async fn test_process_delete_message_malformed_encoded_content() {
        use crate::tester;
        use xmtp_db::group_message::{ContentType, DeliveryStatus, GroupMessageKind};

        tester!(alix);
        let alix_group = alix.create_group(None, None)?;

        // Create a message with completely invalid EncodedContent proto
        let malformed_message = xmtp_db::group_message::StoredGroupMessage {
            id: vec![1, 2, 3],
            group_id: alix_group.group_id,
            decrypted_message_bytes: vec![0xFF, 0xFE, 0xFD], // Invalid protobuf
            sent_at_ns: xmtp_common::time::now_ns(),
            kind: GroupMessageKind::Application,
            sender_installation_id: vec![1, 2, 3],
            sender_inbox_id: alix.inbox_id().to_string(),
            delivery_status: DeliveryStatus::Published,
            content_type: ContentType::DeleteMessage,
            version_major: 1,
            version_minor: 0,
            authority_id: "xmtp.org".to_string(),
            reference_id: None,
            expire_at_ns: None,
            sequence_id: 1,
            envelope_hash: None,
            expiry_ns: None,
            inserted_at_ns: 0,
            should_push: false,
            idempotency_key: String::new(),
        };

        // Use load_mls_group_with_lock to get access to the MLS group and call process_delete_message
        let storage = alix.context.mls_storage();
        let result: Result<(), crate::groups::GroupError> =
            alix_group.load_mls_group_with_lock(storage, |mls_group| {
                let inner_result = alix_group.process_delete_message(
                    &mls_group,
                    storage,
                    &malformed_message,
                    alix_group.context.events(),
                );
                match inner_result {
                    Ok(()) => Ok(()),
                    Err(_) => Err(crate::groups::GroupError::InvalidGroupMembership),
                }
            });

        assert!(
            result.is_ok(),
            "Malformed EncodedContent should not cause error"
        );
    }

    /// Test that process_delete_message handles valid EncodedContent with malformed inner proto
    #[xmtp_common::test(unwrap_try = true)]
    async fn test_process_delete_message_malformed_inner_proto() {
        use crate::tester;
        use prost::Message;
        use xmtp_db::group_message::{ContentType, DeliveryStatus, GroupMessageKind};
        use xmtp_proto::xmtp::mls::message_contents::EncodedContent;

        tester!(alix);
        let alix_group = alix.create_group(None, None)?;

        // Create a valid EncodedContent wrapper but with invalid inner DeleteMessage content
        let encoded_content = EncodedContent {
            r#type: Some(xmtp_proto::xmtp::mls::message_contents::ContentTypeId {
                authority_id: "xmtp.org".to_string(),
                type_id: "deleteMessage".to_string(),
                version_major: 1,
                version_minor: 0,
            }),
            parameters: std::collections::HashMap::new(),
            fallback: None,
            compression: None,
            content: vec![0xFF, 0xFE, 0xFD], // Invalid DeleteMessage proto bytes
        };

        let mut encoded_bytes = Vec::new();
        encoded_content.encode(&mut encoded_bytes)?;

        let malformed_message = xmtp_db::group_message::StoredGroupMessage {
            id: vec![4, 5, 6],
            group_id: alix_group.group_id,
            decrypted_message_bytes: encoded_bytes,
            sent_at_ns: xmtp_common::time::now_ns(),
            kind: GroupMessageKind::Application,
            sender_installation_id: vec![1, 2, 3],
            sender_inbox_id: alix.inbox_id().to_string(),
            delivery_status: DeliveryStatus::Published,
            content_type: ContentType::DeleteMessage,
            version_major: 1,
            version_minor: 0,
            authority_id: "xmtp.org".to_string(),
            reference_id: None,
            expire_at_ns: None,
            sequence_id: 2,
            envelope_hash: None,
            expiry_ns: None,
            inserted_at_ns: 0,
            should_push: false,
            idempotency_key: String::new(),
        };

        let storage = alix.context.mls_storage();
        let result: Result<(), crate::groups::GroupError> =
            alix_group.load_mls_group_with_lock(storage, |mls_group| {
                let inner_result = alix_group.process_delete_message(
                    &mls_group,
                    storage,
                    &malformed_message,
                    alix_group.context.events(),
                );
                match inner_result {
                    Ok(()) => Ok(()),
                    Err(_) => Err(crate::groups::GroupError::InvalidGroupMembership),
                }
            });

        assert!(
            result.is_ok(),
            "Malformed inner DeleteMessage proto should not cause error"
        );
    }

    /// Test that process_delete_message handles invalid hex message_id gracefully
    #[xmtp_common::test(unwrap_try = true)]
    async fn test_process_delete_message_invalid_hex_message_id() {
        use crate::tester;
        use prost::Message;
        use xmtp_db::group_message::{ContentType, DeliveryStatus, GroupMessageKind};
        use xmtp_proto::xmtp::mls::message_contents::EncodedContent;
        use xmtp_proto::xmtp::mls::message_contents::content_types::DeleteMessage;

        tester!(alix);
        let alix_group = alix.create_group(None, None)?;

        // Create a valid DeleteMessage but with invalid hex in message_id
        let delete_msg = DeleteMessage {
            message_id: "not_valid_hex!!!".to_string(), // Invalid hex
        };

        let mut delete_bytes = Vec::new();
        delete_msg.encode(&mut delete_bytes)?;

        let encoded_content = EncodedContent {
            r#type: Some(xmtp_proto::xmtp::mls::message_contents::ContentTypeId {
                authority_id: "xmtp.org".to_string(),
                type_id: "deleteMessage".to_string(),
                version_major: 1,
                version_minor: 0,
            }),
            parameters: std::collections::HashMap::new(),
            fallback: None,
            compression: None,
            content: delete_bytes,
        };

        let mut encoded_bytes = Vec::new();
        encoded_content.encode(&mut encoded_bytes)?;

        let message_with_bad_hex = xmtp_db::group_message::StoredGroupMessage {
            id: vec![7, 8, 9],
            group_id: alix_group.group_id,
            decrypted_message_bytes: encoded_bytes,
            sent_at_ns: xmtp_common::time::now_ns(),
            kind: GroupMessageKind::Application,
            sender_installation_id: vec![1, 2, 3],
            sender_inbox_id: alix.inbox_id().to_string(),
            delivery_status: DeliveryStatus::Published,
            content_type: ContentType::DeleteMessage,
            version_major: 1,
            version_minor: 0,
            authority_id: "xmtp.org".to_string(),
            reference_id: None,
            expire_at_ns: None,
            sequence_id: 3,
            envelope_hash: None,
            expiry_ns: None,
            inserted_at_ns: 0,
            should_push: false,
            idempotency_key: String::new(),
        };

        let storage = alix.context.mls_storage();
        let result: Result<(), crate::groups::GroupError> =
            alix_group.load_mls_group_with_lock(storage, |mls_group| {
                let inner_result = alix_group.process_delete_message(
                    &mls_group,
                    storage,
                    &message_with_bad_hex,
                    alix_group.context.events(),
                );
                match inner_result {
                    Ok(()) => Ok(()),
                    Err(_) => Err(crate::groups::GroupError::InvalidGroupMembership),
                }
            });

        assert!(
            result.is_ok(),
            "Invalid hex message_id should not cause error"
        );
    }

    /// Pin the `CommitResult` mapping for each arm of the AppData-aware
    /// wrapper error so future refactors of `commit_result()` can't
    /// silently reshuffle what a receiver will write to the remote
    /// commit log. In particular: `AppDataDecode` failures must be
    /// `Invalid` (non-retriable wire-format violation), not
    /// `Undecryptable` (retriable transport failure).
    #[test]
    fn process_message_with_app_data_error_commit_result_mapping() {
        use super::super::app_data::ProcessMessageWithAppDataError;
        use openmls::group::ValidationError;
        use xmtp_mls_common::app_data::component_source::ComponentSourceError;

        let wrong_epoch = GroupMessageProcessingError::OpenMlsProcessMessageWithAppData(
            ProcessMessageWithAppDataError::OpenMls(ProcessMessageError::ValidationError(
                ValidationError::WrongEpoch,
            )),
        );
        assert_eq!(wrong_epoch.commit_result(), CommitResult::WrongEpoch);

        let other_openmls = GroupMessageProcessingError::OpenMlsProcessMessageWithAppData(
            ProcessMessageWithAppDataError::OpenMls(ProcessMessageError::IncompatibleWireFormat),
        );
        assert_eq!(other_openmls.commit_result(), CommitResult::Undecryptable);

        // AppData decode failures are deterministic wire-format violations:
        // retrying the same bytes can't fix them, and the receiver should
        // log them as `Invalid` rather than `Undecryptable`.
        let decode_failure = GroupMessageProcessingError::OpenMlsProcessMessageWithAppData(
            ProcessMessageWithAppDataError::AppDataDecode(ComponentSourceError::UnknownComponent(
                xmtp_mls_common::app_data::component_id::ComponentId::from(0u16),
            )),
        );
        assert_eq!(decode_failure.commit_result(), CommitResult::Invalid);
        assert!(
            !decode_failure.is_retryable(),
            "wire-format violations must not be retriable"
        );
    }
}

/// Collects events that should be sent after database transactions complete
#[derive(Default)]
pub struct DeferredEvents {
    worker_events: VecDeque<SyncWorkerEvent>,
    /// Workers to wake once the txn commits. Post-commit (not inline) is load-bearing:
    /// a pre-commit nudge can race a worker's DB read and be lost, parking the work.
    wake_workers: HashSet<WorkerKind>,
}

impl DeferredEvents {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_worker_event(&mut self, event: SyncWorkerEvent) {
        self.worker_events.push_back(event);
    }

    /// Request a post-commit wake of `kind`. Idempotent within a txn.
    pub fn wake_worker(&mut self, kind: WorkerKind) {
        self.wake_workers.insert(kind);
    }

    /// Send all collected events to their respective channels
    pub fn send_all<Context: XmtpSharedContext>(&mut self, context: &Context) {
        while let Some(event) = self.worker_events.pop_front() {
            let _ = context.worker_events().send(event);
        }

        for kind in self.wake_workers.drain() {
            // Never nudge a disabled worker — it won't drain the signal.
            if !context.worker_config().worker_enabled(kind) {
                continue;
            }
            match kind {
                WorkerKind::DisappearingMessages => context.disappearing_channels().rearm(),
                WorkerKind::TaskRunner => context.task_channels().wake(),
                // Other workers have no post-commit wake channel today.
                _ => {}
            }
        }
    }
}
