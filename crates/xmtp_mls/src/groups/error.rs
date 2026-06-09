use super::group_permissions::GroupMutablePermissionsError;
use super::mls_sync::GroupMessageProcessingError;
use super::summary::SyncSummary;
use super::{intents::IntentError, validated_commit::CommitValidationError};
use crate::identity::IdentityError;
use crate::mls_store::MlsStoreError;
use crate::worker::device_sync::DeviceSyncError;
use crate::{
    client::ClientError, identity_updates::InstallationDiffError, intents::ProcessIntentError,
    subscriptions::LocalEventError,
};
use openmls::{
    error::LibraryError,
    group::{
        CommitToPendingProposalsError, CreateGroupContextExtProposalError, ProposalError,
        ProposeAddMemberError, ProposeRemoveMemberError,
    },
    prelude::{BasicCredentialError, Error as TlsCodecError},
};
use std::collections::HashSet;
use thiserror::Error;
use xmtp_common::ErrorCode;
use xmtp_common::Retryable;
use xmtp_common::retry::RetryableError;
use xmtp_content_types::CodecError;
use xmtp_cryptography::signature::IdentifierValidationError;
use xmtp_db::NotFound;
use xmtp_db::sql_key_store;
use xmtp_mls_common::group_metadata::GroupMetadataError;
use xmtp_mls_common::group_mutable_metadata::GroupMutableMetadataError;
use xmtp_mls_common::mls_ext::payload_encryption::{UnwrapPayloadError, WrapPayloadError};

/// Wraps multiple message processing errors from a single receive operation.
///
/// Contains a list of message IDs that failed and their corresponding errors. May be retryable.
#[derive(Error, Debug, ErrorCode)]
#[error_code(internal)]
pub struct ReceiveErrors {
    /// list of message ids we received
    ids: Vec<u64>,
    errors: Vec<GroupMessageProcessingError>,
}

impl RetryableError for ReceiveErrors {
    fn is_retryable(&self) -> bool {
        self.errors.iter().any(|e| e.is_retryable())
    }
}

impl ReceiveErrors {
    pub fn new(errors: Vec<GroupMessageProcessingError>, ids: Vec<u64>) -> Self {
        Self { ids, errors }
    }
}

impl std::fmt::Display for ReceiveErrors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let errs: HashSet<String> = self.errors.iter().map(|e| e.to_string()).collect();
        let mut sorted = self.ids.clone();
        sorted.sort();
        writeln!(
            f,
            "\n=========================== Receive Errors  =====================\n\
            total of [{}] errors processing [{}] messages in cursor range [{:?} ... {:?}]\n\
            [{}] unique errors:",
            self.errors.len(),
            self.ids.len(),
            sorted.first(),
            sorted.last(),
            errs.len(),
        )?;
        for err in errs.iter() {
            writeln!(f, "{}", err)?;
        }
        writeln!(
            f,
            "================================================================="
        )?;
        Ok(())
    }
}
#[derive(Debug, Error, ErrorCode, Retryable)]
pub enum GroupError {
    #[error(transparent)]
    #[error_code(inherit)]
    NotFound(#[from] NotFound),
    /// Max user limit exceeded.
    ///
    /// Attempted to add too many members. Not retryable.
    #[error("Max user limit exceeded.")]
    UserLimitExceeded,
    /// Sequence ID not found.
    ///
    /// Missing sequence ID in local database. Not retryable.
    #[error("SequenceId not found in local db")]
    MissingSequenceId,
    /// Addresses not found.
    ///
    /// Specified addresses have no XMTP identity. Not retryable.
    #[error("Addresses not found {0:?}")]
    AddressNotFound(Vec<String>),
    /// API error.
    ///
    /// Network request failed. Retryable.
    #[error("api error: {0}")]
    #[retry(inherit)]
    WrappedApi(#[from] xmtp_api::ApiError),
    /// Invalid group membership.
    ///
    /// Group membership state is invalid. Not retryable.
    #[error("invalid group membership")]
    InvalidGroupMembership,
    /// Leave cannot be processed.
    ///
    /// Group leave validation failed. Not retryable.
    #[error(transparent)]
    #[retry(inherit)]
    LeaveCantProcessed(#[from] GroupLeaveValidationError),
    /// Storage error.
    ///
    /// Database operation failed. May be retryable.
    #[error("storage error: {0}")]
    #[retry(inherit)]
    Storage(#[from] xmtp_db::StorageError),
    /// Intent error.
    ///
    /// Failed to process group intent. Not retryable.
    #[error("intent error: {0}")]
    Intent(#[from] IntentError),
    /// Create message error.
    ///
    /// MLS message creation failed. Not retryable.
    #[error("create message: {0}")]
    CreateMessage(#[from] openmls::prelude::CreateMessageError),
    /// TLS codec error.
    ///
    /// MLS TLS encoding/decoding failed. Not retryable.
    #[error("TLS Codec error: {0}")]
    TlsError(#[from] TlsCodecError),
    /// Update group membership error.
    ///
    /// Failed to update group membership. May be retryable.
    #[error("add members: {0}")]
    #[retry(inherit)]
    UpdateGroupMembership(
        #[from] openmls::prelude::UpdateGroupMembershipError<sql_key_store::SqlKeyStoreError>,
    ),
    /// Group create error.
    ///
    /// MLS group creation failed. May be retryable.
    #[error("group create: {0}")]
    #[retry(inherit)]
    GroupCreate(#[from] openmls::group::NewGroupError<sql_key_store::SqlKeyStoreError>),
    /// Self update error.
    ///
    /// MLS self-update operation failed. May be retryable.
    #[error("self update: {0}")]
    #[retry(inherit)]
    SelfUpdate(#[from] openmls::group::SelfUpdateError<sql_key_store::SqlKeyStoreError>),
    /// Welcome error.
    ///
    /// Processing MLS welcome message failed. May be retryable.
    #[error("welcome error: {0}")]
    #[retry(inherit)]
    WelcomeError(#[from] openmls::prelude::WelcomeError<sql_key_store::SqlKeyStoreError>),
    /// Invalid extension.
    ///
    /// MLS extension validation failed. Not retryable.
    #[error("Invalid extension {0}")]
    InvalidExtension(#[from] openmls::prelude::InvalidExtensionError),
    /// Invalid signature.
    ///
    /// MLS signature verification failed. Not retryable.
    #[error("Invalid signature: {0}")]
    Signature(#[from] openmls::prelude::SignatureError),
    /// Client error.
    ///
    /// Client operation failed within group. May be retryable.
    #[error("client: {0}")]
    #[retry(inherit)]
    Client(#[from] ClientError),
    /// Receive error.
    ///
    /// Processing received group message failed. May be retryable.
    #[error("receive error: {0}")]
    #[retry(inherit)]
    ReceiveError(#[from] GroupMessageProcessingError),
    /// Receive errors.
    ///
    /// Multiple message processing failures. May be retryable.
    #[error("Receive errors: {0}")]
    #[retry(inherit)]
    ReceiveErrors(ReceiveErrors),
    /// Address validation error.
    ///
    /// An address/identifier is invalid. Not retryable.
    #[error(transparent)]
    AddressValidation(#[from] IdentifierValidationError),
    /// Local event error.
    ///
    /// Failed to process local event. Not retryable.
    #[error(transparent)]
    #[retry(inherit)]
    LocalEvent(#[from] LocalEventError),
    /// Invalid public keys.
    ///
    /// Keys are not valid Ed25519 public keys. Not retryable.
    #[error("Public Keys {0:?} are not valid ed25519 public keys")]
    InvalidPublicKeys(Vec<Vec<u8>>),
    /// Commit validation error.
    ///
    /// MLS commit validation failed. May be retryable.
    #[error("Commit validation error {0}")]
    #[retry(inherit)]
    CommitValidation(#[from] CommitValidationError),
    /// Identity error.
    ///
    /// Identity operation failed. Not retryable.
    #[error("identity error: {0}")]
    #[retry(inherit)]
    Identity(#[from] IdentityError),
    /// Conversion error.
    ///
    /// Proto conversion failed. Not retryable.
    #[error("conversion error: {0}")]
    ConversionError(#[from] xmtp_proto::ConversionError),
    /// Crypto error.
    ///
    /// Cryptographic operation failed. Not retryable.
    #[error("crypto error: {0}")]
    CryptoError(#[from] openmls::prelude::CryptoError),
    /// Group context proposal error.
    ///
    /// Failed to create group context extension proposal. May be retryable.
    #[error("create group context proposal error: {0}")]
    #[retry(inherit)]
    CreateGroupContextExtProposalError(
        #[from] CreateGroupContextExtProposalError<sql_key_store::SqlKeyStoreError>,
    ),
    /// Propose add member error.
    ///
    /// Failed to create an add-member proposal. May be retryable.
    #[error("propose add member error: {0}")]
    #[retry(inherit)]
    ProposeAddMember(#[from] ProposeAddMemberError<sql_key_store::SqlKeyStoreError>),
    /// Propose remove member error.
    ///
    /// Failed to create a remove-member proposal. May be retryable.
    #[error("propose remove member error: {0}")]
    #[retry(inherit)]
    ProposeRemoveMember(#[from] ProposeRemoveMemberError<sql_key_store::SqlKeyStoreError>),
    /// Proposal error.
    ///
    /// Generic MLS proposal creation/handling failure. May be retryable.
    #[error("proposal error: {0}")]
    #[retry(inherit)]
    Proposal(#[from] ProposalError<sql_key_store::SqlKeyStoreError>),
    /// Commit to pending proposals error.
    ///
    /// Failed to commit pending proposals into an MLS commit. May be retryable.
    #[error("commit to pending proposals error: {0}")]
    #[retry(inherit)]
    CommitToPendingProposals(
        #[from] CommitToPendingProposalsError<sql_key_store::SqlKeyStoreError>,
    ),
    /// Merge pending commit error.
    ///
    /// Failed to merge a pending commit into local state. May be retryable.
    #[error("merge pending commit error: {0}")]
    #[retry(inherit)]
    MergePendingCommit(
        #[from] openmls::group::MergePendingCommitError<sql_key_store::SqlKeyStoreError>,
    ),
    /// Proposals not supported.
    ///
    /// Encountered a proposal when our client does not support proposals. Not retryable.
    #[error("Proposals not supported: {0}")]
    ProposalsNotSupported(String),
    /// Caller asked to set `MIN_SUPPORTED_PROTOCOL_VERSION` to a value
    /// the caller's own client does not satisfy. Refusing prevents the
    /// caller from immediately pausing themselves (and every peer at or
    /// below their version) the moment the bump lands. Not retryable.
    #[error("min_version {requested} exceeds own pkg_version {own}")]
    MinVersionExceedsOwnVersion { requested: String, own: String },
    /// Caller asked to lower `MIN_SUPPORTED_PROTOCOL_VERSION` below the
    /// floor already on the group. Monotonic-only: a downgrade would
    /// silently unpause peers between the old and new floors, defeating
    /// the gate. Not retryable.
    #[error("min_version {requested} would downgrade existing floor {current}")]
    MinVersionDowngrade { requested: String, current: String },
    /// Caller passed a `min_version` string that doesn't parse as
    /// semver. Surfaces from the send-side paths
    /// (`enable_proposals`, `update_group_min_version`) so SDK
    /// consumers can `match`-handle malformed input without parsing
    /// string-flattened wrappers. Not retryable.
    #[error("invalid min_version {value:?}: {reason}")]
    InvalidMinVersion { value: String, reason: String },
    /// Component source error.
    ///
    /// Failed to encode, decode, or look up a well-known component during the
    /// AppDataUpdate path. Not retryable.
    #[error("component source error: {0}")]
    ComponentSource(#[from] super::app_data::component_source::ComponentSourceError),
    /// AppData commit error.
    ///
    /// Failed to build or stage a commit that bundles an inline AppDataUpdate
    /// proposal. Wraps the structured `GroupAppDataError` from
    /// [`stage_app_data_propose_and_commit`] so the underlying OpenMLS create/stage
    /// failure is preserved instead of being string-flattened.
    #[error("app data commit error: {0}")]
    #[retry(inherit)]
    AppDataCommit(#[from] super::app_data::GroupAppDataError<sql_key_store::SqlKeyStoreError>),
    /// Bootstrap synthesis failure — sender-side couldn't build the
    /// complete set of initial component values for the migration
    /// commit. Includes identity-update lookup failures.
    ///
    /// Conditionally retryable: delegates to the wrapped
    /// [`BootstrapSynthesisError`], which retries only when an inner
    /// identity-update API error is itself retryable. Decode/registry-shape
    /// failures are deterministic and not retryable.
    #[error("bootstrap synthesis error: {0}")]
    // Bootstrap synthesis can fail on a transient identity-update
    // API blip — delegate to the inner error so we retry on
    // network errors and stay non-retryable on deterministic
    // wire-format / registry-shape failures.
    #[retry(inherit)]
    BootstrapSynthesis(#[from] super::app_data::migration::BootstrapSynthesisError),
    /// Bootstrap commit-build failure.
    ///
    /// Not retryable: every variant of [`BootstrapCommitError`] is a
    /// deterministic OpenMLS commit failure, a TLS codec error, or a
    /// caller-side precondition violation.
    #[error("bootstrap commit error: {0}")]
    BootstrapCommit(
        #[from] super::app_data::migration::BootstrapCommitError<sql_key_store::SqlKeyStoreError>,
    ),
    /// Credential error.
    ///
    /// MLS credential validation failed. Not retryable.
    #[error("Credential error")]
    CredentialError(#[from] BasicCredentialError),
    /// Leaf node error.
    ///
    /// MLS leaf node operation failed. Not retryable.
    #[error("LeafNode error")]
    LeafNodeError(#[from] LibraryError),
    /// Installation diff error.
    ///
    /// Installation diff computation failed. May be retryable.
    #[error("Installation diff error: {0}")]
    #[retry(inherit)]
    InstallationDiff(#[from] InstallationDiffError),
    /// No PSK support.
    ///
    /// Pre-shared keys are not supported. Not retryable.
    #[error("PSKs are not support")]
    NoPSKSupport,
    /// SQL key store error.
    ///
    /// OpenMLS key store operation failed. May be retryable.
    #[error("sql key store error: {0}")]
    #[retry(inherit)]
    SqlKeyStore(#[from] sql_key_store::SqlKeyStoreError),
    /// Sync failed to wait.
    ///
    /// Waiting for intent sync failed. Retryable.
    #[error("Sync failed to wait for intent: {}", _0)]
    #[retry(true)]
    SyncFailedToWait(Box<SyncSummary>),
    /// Missing pending commit.
    ///
    /// Expected pending commit not found. Not retryable.
    #[error("Missing pending commit")]
    MissingPendingCommit,
    /// Process intent error.
    ///
    /// Failed to process group intent. May be retryable.
    #[error(transparent)]
    #[retry(inherit)]
    ProcessIntent(#[from] ProcessIntentError),
    /// Failed to load lock.
    ///
    /// Concurrency lock acquisition failed. Retryable.
    #[error("Failed to load lock")]
    #[retry(true)]
    LockUnavailable,
    /// Exceeded max characters.
    ///
    /// Field value exceeds character limit. Not retryable.
    #[error("Exceeded max characters for this field. Must be under: {length}")]
    TooManyCharacters { length: usize },
    /// Group paused until update.
    ///
    /// Group is paused until a newer version is available. Not retryable.
    #[error("Group is paused until version {0} is available")]
    GroupPausedUntilUpdate(String),
    /// Group is inactive.
    ///
    /// Operation on an inactive group. Not retryable.
    #[error("Group is inactive")]
    GroupInactive,
    /// Sync summary.
    ///
    /// Sync operation completed with errors. May be retryable.
    #[error("{}", _0.to_string())]
    #[retry(inherit)]
    Sync(#[from] Box<SyncSummary>),
    /// Database connection error.
    ///
    /// Database connection failed. Retryable.
    #[error(transparent)]
    #[retry(inherit)]
    Db(#[from] xmtp_db::ConnectionError),
    /// MLS store error.
    ///
    /// OpenMLS key store failed. Not retryable.
    #[error(transparent)]
    #[retry(inherit)]
    MlsStore(#[from] MlsStoreError),
    /// Metadata permissions error.
    ///
    /// Metadata permission check failed. Not retryable.
    #[error(transparent)]
    #[retry(inherit)]
    MetadataPermissionsError(#[from] MetadataPermissionsError),
    /// Failed to verify installations.
    ///
    /// Installation verification failed. Not retryable.
    #[error("Failed to verify all installations")]
    FailedToVerifyInstallations,
    /// No welcomes to send.
    ///
    /// No welcome messages to send to new members. Not retryable.
    #[error("no welcomes to send")]
    NoWelcomesToSend,
    /// Codec error.
    ///
    /// Content type codec failed. Retryable.
    #[error("Codec error: {0}")]
    #[retry(true)]
    CodecError(#[from] CodecError),
    /// Wrap welcome error.
    ///
    /// Failed to wrap welcome message. Not retryable.
    #[error(transparent)]
    #[retry(inherit)]
    WrapWelcome(#[from] WrapPayloadError),
    /// Unwrap welcome error.
    ///
    /// Failed to unwrap welcome message. Not retryable.
    #[error(transparent)]
    #[retry(inherit)]
    UnwrapWelcome(#[from] UnwrapPayloadError),
    /// Welcome data not found.
    ///
    /// Welcome data missing from topic. Not retryable.
    #[error("Failed to retrieve welcome data from topic {0}")]
    WelcomeDataNotFound(String),
    /// Result not initialized.
    ///
    /// Expected result was not initialized. Not retryable.
    #[error("Result was not initialized")]
    UninitializedResult,
    /// Diesel ORM error.
    ///
    /// Raw database query failed. May be retryable.
    #[error(transparent)]
    #[retry(inherit)]
    Diesel(#[from] xmtp_db::diesel::result::Error),
    /// Uninitialized field.
    ///
    /// Builder field not initialized. Not retryable.
    #[error(transparent)]
    UninitializedField(#[from] derive_builder::UninitializedFieldError),
    /// Delete message error.
    ///
    /// Failed to delete message. Not retryable.
    #[error(transparent)]
    #[retry(inherit)]
    DeleteMessage(#[from] DeleteMessageError),
    /// Device sync error.
    ///
    /// Device sync operation failed. May be retryable.
    #[error(transparent)]
    #[retry(inherit)]
    DeviceSync(#[from] Box<DeviceSyncError>),
}

#[derive(Error, Debug, Retryable)]
pub enum DeleteMessageError {
    #[error("Message not found: {0}")]
    MessageNotFound(String),
    #[error("Not authorized to delete this message")]
    NotAuthorized,
    #[error("Cannot delete this message type")]
    NonDeletableMessage,
    #[error("Message already deleted")]
    MessageAlreadyDeleted,
}

impl From<prost::EncodeError> for GroupError {
    fn from(value: prost::EncodeError) -> Self {
        GroupError::ConversionError(value.into())
    }
}

impl From<prost::DecodeError> for GroupError {
    fn from(value: prost::DecodeError) -> Self {
        GroupError::ConversionError(value.into())
    }
}

impl From<SyncSummary> for GroupError {
    fn from(value: SyncSummary) -> Self {
        GroupError::Sync(Box::new(value))
    }
}

#[derive(Error, Debug, Retryable)]
pub enum MetadataPermissionsError {
    #[error(transparent)]
    Permissions(#[from] GroupMutablePermissionsError),
    #[error(transparent)]
    Mutable(#[from] GroupMutableMetadataError),
    #[error("Metadata error {0}")]
    GroupMetadata(#[from] GroupMetadataError),
    #[error("Metadata update must specify a metadata field")]
    InvalidPermissionUpdate,
    #[error("cannot change metadata of DM")]
    DmGroupMetadataForbidden,
    #[error(transparent)]
    DmValidation(#[from] DmValidationError),
    #[error("Invalid extension: {0}")]
    InvalidExtension(#[from] openmls::prelude::InvalidExtensionError),
    /// Failed to decode a well-known component value from the
    /// AppData dictionary on a migrated group. Surfaces
    /// [`ComponentSourceError`] via `#[from]` so callers (e.g.
    /// `mutable_metadata()`, `metadata()`) preserve the structured
    /// source.
    #[error(transparent)]
    ComponentSource(#[from] crate::groups::app_data::component_source::ComponentSourceError),
}

#[derive(Error, Debug, Retryable)]
pub enum GroupLeaveValidationError {
    #[error("cannot leave a DM conversation")]
    DmLeaveForbidden,
    #[error("cannot leave a group that has only one member")]
    SingleMemberLeaveRejected,
    #[error("super-admin cannot leave a group; must be demoted first")]
    SuperAdminLeaveForbidden,
    #[error("inbox ID already exists in the pending leave list")]
    InboxAlreadyInPendingList,
    #[error("inbox ID does not exist in the pending leave list")]
    InboxNotInPendingList,
    #[error("only a member of the group can send a leave request or retract a leave request")]
    NotAGroupMember,
}

#[derive(Error, Debug, Retryable)]
pub enum DmValidationError {
    #[error("DM group must have DmMembers set")]
    OurInboxMustBeMember,
    #[error("DM group must have our inbox as one of the dm members")]
    MustHaveMembersSet,
    #[error("Invalid conversation type for DM group")]
    InvalidConversationType,
    #[error("DM members do not match expected inboxes")]
    ExpectedInboxesDoNotMatch,
    #[error("DM group must have empty admin and super admin lists")]
    MustHaveEmptyAdminAndSuperAdmin,
    #[error("Invalid permissions for DM group")]
    InvalidPermissions,
}

impl crate::worker::NeedsDbReconnect for GroupError {
    /// Forwards a dropped-pool signal from storage-bearing variants so a worker
    /// catching `GroupError`s per item can stop on disconnect; else `false`.
    fn needs_db_reconnect(&self) -> bool {
        match self {
            Self::Storage(s) => s.db_needs_connection(),
            Self::Client(c) => c.db_needs_connection(),
            Self::Db(c) => c.db_needs_connection(),
            Self::MlsStore(s) => s.needs_db_reconnect(),
            Self::Identity(i) => i.needs_db_reconnect(),
            Self::DeviceSync(d) => d.needs_db_reconnect(),
            _ => false,
        }
    }
}
