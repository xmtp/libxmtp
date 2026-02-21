use super::group_permissions::GroupMutablePermissionsError;
use super::mls_ext::{UnwrapWelcomeError, WrapWelcomeError};
use super::mls_sync::GroupMessageProcessingError;
use super::summary::SyncSummary;
use super::{intents::IntentError, validated_commit::CommitValidationError};
use crate::groups::device_sync::DeviceSyncError;
use crate::identity::IdentityError;
use crate::mls_store::MlsStoreError;
use crate::{
    client::ClientError, identity_updates::InstallationDiffError, intents::ProcessIntentError,
    subscriptions::LocalEventError,
};
use openmls::{
    error::LibraryError,
    group::CreateGroupContextExtProposalError,
    prelude::{BasicCredentialError, Error as TlsCodecError},
};
use std::collections::HashSet;
use thiserror::Error;
use xmtp_common::ErrorCode;
use xmtp_common::retry::RetryableError;
use xmtp_content_types::CodecError;
use xmtp_cryptography::signature::IdentifierValidationError;
use xmtp_db::NotFound;
use xmtp_db::sql_key_store;
use xmtp_mls_common::group_metadata::GroupMetadataError;
use xmtp_mls_common::group_mutable_metadata::GroupMutableMetadataError;

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
#[derive(Debug, Error, ErrorCode)]
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
    LeaveCantProcessed(#[from] GroupLeaveValidationError),
    /// Storage error.
    ///
    /// Database operation failed. May be retryable.
    #[error("storage error: {0}")]
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
    UpdateGroupMembership(
        #[from] openmls::prelude::UpdateGroupMembershipError<sql_key_store::SqlKeyStoreError>,
    ),
    /// Group create error.
    ///
    /// MLS group creation failed. May be retryable.
    #[error("group create: {0}")]
    GroupCreate(#[from] openmls::group::NewGroupError<sql_key_store::SqlKeyStoreError>),
    /// Self update error.
    ///
    /// MLS self-update operation failed. May be retryable.
    #[error("self update: {0}")]
    SelfUpdate(#[from] openmls::group::SelfUpdateError<sql_key_store::SqlKeyStoreError>),
    /// Welcome error.
    ///
    /// Processing MLS welcome message failed. May be retryable.
    #[error("welcome error: {0}")]
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
    Client(#[from] ClientError),
    /// Receive error.
    ///
    /// Processing received group message failed. May be retryable.
    #[error("receive error: {0}")]
    ReceiveError(#[from] GroupMessageProcessingError),
    /// Receive errors.
    ///
    /// Multiple message processing failures. May be retryable.
    #[error("Receive errors: {0}")]
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
    CommitValidation(#[from] CommitValidationError),
    /// Identity error.
    ///
    /// Identity operation failed. Not retryable.
    #[error("identity error: {0}")]
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
    CreateGroupContextExtProposalError(
        #[from] CreateGroupContextExtProposalError<sql_key_store::SqlKeyStoreError>,
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
    SqlKeyStore(#[from] sql_key_store::SqlKeyStoreError),
    /// Sync failed to wait.
    ///
    /// Waiting for intent sync failed. Retryable.
    #[error("Sync failed to wait for intent")]
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
    ProcessIntent(#[from] ProcessIntentError),
    /// Failed to load lock.
    ///
    /// Concurrency lock acquisition failed. Retryable.
    #[error("Failed to load lock")]
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
    Sync(#[from] Box<SyncSummary>),
    /// Database connection error.
    ///
    /// Database connection failed. Retryable.
    #[error(transparent)]
    Db(#[from] xmtp_db::ConnectionError),
    /// MLS store error.
    ///
    /// OpenMLS key store failed. Not retryable.
    #[error(transparent)]
    MlsStore(#[from] MlsStoreError),
    /// Metadata permissions error.
    ///
    /// Metadata permission check failed. Not retryable.
    #[error(transparent)]
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
    CodecError(#[from] CodecError),
    /// Wrap welcome error.
    ///
    /// Failed to wrap welcome message. Not retryable.
    #[error(transparent)]
    WrapWelcome(#[from] WrapWelcomeError),
    /// Unwrap welcome error.
    ///
    /// Failed to unwrap welcome message. Not retryable.
    #[error(transparent)]
    UnwrapWelcome(#[from] UnwrapWelcomeError),
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
    DeleteMessage(#[from] DeleteMessageError),
    /// Device sync error.
    ///
    /// Device sync operation failed. May be retryable.
    #[error(transparent)]
    DeviceSync(#[from] Box<DeviceSyncError>),
}

#[derive(Error, Debug)]
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

impl RetryableError for DeleteMessageError {
    fn is_retryable(&self) -> bool {
        false
    }
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

#[derive(Error, Debug)]
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
}

impl RetryableError for MetadataPermissionsError {
    fn is_retryable(&self) -> bool {
        false
    }
}

#[derive(Error, Debug)]
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

impl RetryableError for GroupLeaveValidationError {
    fn is_retryable(&self) -> bool {
        false
    }
}

#[derive(Error, Debug)]
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

impl RetryableError for DmValidationError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::OurInboxMustBeMember
            | Self::MustHaveMembersSet
            | Self::InvalidConversationType
            | Self::ExpectedInboxesDoNotMatch
            | Self::MustHaveEmptyAdminAndSuperAdmin
            | Self::InvalidPermissions => false,
        }
    }
}

impl RetryableError for GroupError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::ReceiveErrors(errors) => errors.is_retryable(),
            Self::Client(client_error) => client_error.is_retryable(),
            Self::Storage(storage) => storage.is_retryable(),
            Self::ReceiveError(msg) => msg.is_retryable(),
            Self::Identity(identity) => identity.is_retryable(),
            Self::UpdateGroupMembership(update) => update.is_retryable(),
            Self::GroupCreate(group) => group.is_retryable(),
            Self::SelfUpdate(update) => update.is_retryable(),
            Self::WelcomeError(welcome) => welcome.is_retryable(),
            Self::SqlKeyStore(sql) => sql.is_retryable(),
            Self::InstallationDiff(diff) => diff.is_retryable(),
            Self::CreateGroupContextExtProposalError(create) => create.is_retryable(),
            Self::CommitValidation(err) => err.is_retryable(),
            Self::WrappedApi(err) => err.is_retryable(),
            Self::ProcessIntent(err) => err.is_retryable(),
            Self::LocalEvent(err) => err.is_retryable(),
            Self::LockUnavailable => true,
            Self::SyncFailedToWait(_) => true,
            Self::CodecError(_) => true,
            Self::Sync(s) => s.is_retryable(),
            Self::Db(e) => e.is_retryable(),
            Self::MlsStore(e) => e.is_retryable(),
            Self::MetadataPermissionsError(e) => e.is_retryable(),
            Self::WrapWelcome(e) => e.is_retryable(),
            Self::UnwrapWelcome(e) => e.is_retryable(),
            Self::Diesel(e) => e.is_retryable(),
            Self::LeaveCantProcessed(e) => e.is_retryable(),
            Self::DeleteMessage(e) => e.is_retryable(),
            Self::DeviceSync(e) => e.is_retryable(),
            Self::NotFound(_)
            | Self::UserLimitExceeded
            | Self::InvalidGroupMembership
            | Self::Intent(_)
            | Self::CreateMessage(_)
            | Self::TlsError(_)
            | Self::MissingSequenceId
            | Self::AddressNotFound(_)
            | Self::InvalidExtension(_)
            | Self::Signature(_)
            | Self::LeafNodeError(_)
            | Self::NoPSKSupport
            | Self::MissingPendingCommit
            | Self::AddressValidation(_)
            | Self::InvalidPublicKeys(_)
            | Self::CredentialError(_)
            | Self::ConversionError(_)
            | Self::CryptoError(_)
            | Self::TooManyCharacters { .. }
            | Self::GroupPausedUntilUpdate(_)
            | Self::GroupInactive
            | Self::FailedToVerifyInstallations
            | Self::NoWelcomesToSend
            | Self::WelcomeDataNotFound(_)
            | Self::UninitializedField(_)
            | Self::UninitializedResult => false,
        }
    }
}
