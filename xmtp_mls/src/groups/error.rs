use super::group_permissions::GroupMutablePermissionsError;
use super::mls_ext::{UnwrapWelcomeError, WrapWelcomeError};
use super::mls_sync::GroupMessageProcessingError;
use super::summary::SyncSummary;
use super::{intents::IntentError, validated_commit::CommitValidationError};
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
use xmtp_common::retry::RetryableError;
use xmtp_cryptography::signature::IdentifierValidationError;
use xmtp_db::sql_key_store;
use xmtp_db::NotFound;
use xmtp_mls_common::group_metadata::GroupMetadataError;
use xmtp_mls_common::group_mutable_metadata::GroupMutableMetadataError;

#[derive(Error, Debug)]
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
#[derive(Debug, Error)]
pub enum GroupError {
    #[error(transparent)]
    NotFound(#[from] NotFound),
    #[error("Max user limit exceeded.")]
    UserLimitExceeded,
    #[error("SequenceId not found in local db")]
    MissingSequenceId,
    #[error("Addresses not found {0:?}")]
    AddressNotFound(Vec<String>),
    #[error("api error: {0}")]
    WrappedApi(#[from] xmtp_api::ApiError),
    #[error("invalid group membership")]
    InvalidGroupMembership,
    #[error("storage error: {0}")]
    Storage(#[from] xmtp_db::StorageError),
    #[error("intent error: {0}")]
    Intent(#[from] IntentError),
    #[error("create message: {0}")]
    CreateMessage(#[from] openmls::prelude::CreateMessageError),
    #[error("TLS Codec error: {0}")]
    TlsError(#[from] TlsCodecError),
    #[error("add members: {0}")]
    UpdateGroupMembership(
        #[from] openmls::prelude::UpdateGroupMembershipError<sql_key_store::SqlKeyStoreError>,
    ),
    #[error("group create: {0}")]
    GroupCreate(#[from] openmls::group::NewGroupError<sql_key_store::SqlKeyStoreError>),
    #[error("self update: {0}")]
    SelfUpdate(#[from] openmls::group::SelfUpdateError<sql_key_store::SqlKeyStoreError>),
    #[error("welcome error: {0}")]
    WelcomeError(#[from] openmls::prelude::WelcomeError<sql_key_store::SqlKeyStoreError>),
    #[error("Invalid extension {0}")]
    InvalidExtension(#[from] openmls::prelude::InvalidExtensionError),
    #[error("Invalid signature: {0}")]
    Signature(#[from] openmls::prelude::SignatureError),
    #[error("client: {0}")]
    Client(#[from] ClientError),
    #[error("receive error: {0}")]
    ReceiveError(#[from] GroupMessageProcessingError),
    #[error("Receive errors: {0}")]
    ReceiveErrors(ReceiveErrors),
    #[error(transparent)]
    AddressValidation(#[from] IdentifierValidationError),
    #[error(transparent)]
    LocalEvent(#[from] LocalEventError),
    #[error("Public Keys {0:?} are not valid ed25519 public keys")]
    InvalidPublicKeys(Vec<Vec<u8>>),
    #[error("Commit validation error {0}")]
    CommitValidation(#[from] CommitValidationError),
    #[error("identity error: {0}")]
    Identity(#[from] IdentityError),
    #[error("serialization error: {0}")]
    EncodeError(#[from] prost::EncodeError),
    #[error("create group context proposal error: {0}")]
    CreateGroupContextExtProposalError(
        #[from] CreateGroupContextExtProposalError<sql_key_store::SqlKeyStoreError>,
    ),
    #[error("Credential error")]
    CredentialError(#[from] BasicCredentialError),
    #[error("LeafNode error")]
    LeafNodeError(#[from] LibraryError),
    #[error("Installation diff error: {0}")]
    InstallationDiff(#[from] InstallationDiffError),
    #[error("PSKs are not support")]
    NoPSKSupport,
    #[error("sql key store error: {0}")]
    SqlKeyStore(#[from] sql_key_store::SqlKeyStoreError),
    #[error("Sync failed to wait for intent")]
    SyncFailedToWait(Box<SyncSummary>),
    #[error("Missing pending commit")]
    MissingPendingCommit,
    #[error(transparent)]
    ProcessIntent(#[from] ProcessIntentError),
    #[error("Failed to load lock")]
    LockUnavailable,
    #[error("Exceeded max characters for this field. Must be under: {length}")]
    TooManyCharacters { length: usize },
    #[error("Group is paused until version {0} is available")]
    GroupPausedUntilUpdate(String),
    #[error("Group is inactive")]
    GroupInactive,
    #[error("{}", _0.to_string())]
    Sync(#[from] Box<SyncSummary>),
    #[error(transparent)]
    Db(#[from] xmtp_db::ConnectionError),
    #[error(transparent)]
    MlsStore(#[from] MlsStoreError),
    #[error(transparent)]
    MetadataPermissionsError(#[from] MetadataPermissionsError),
    #[error("Failed to verify all installations")]
    FailedToVerifyInstallations,
    #[error("no welcomes to send")]
    NoWelcomesToSend,
    #[error(transparent)]
    WrapWelcome(#[from] WrapWelcomeError),
    #[error(transparent)]
    UnwrapWelcome(#[from] UnwrapWelcomeError),
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
            Self::Sync(s) => s.is_retryable(),
            Self::Db(e) => e.is_retryable(),
            Self::MlsStore(e) => e.is_retryable(),
            Self::MetadataPermissionsError(e) => e.is_retryable(),
            Self::WrapWelcome(e) => e.is_retryable(),
            Self::UnwrapWelcome(e) => e.is_retryable(),
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
            | Self::EncodeError(_)
            | Self::TooManyCharacters { .. }
            | Self::GroupPausedUntilUpdate(_)
            | Self::GroupInactive
            | Self::FailedToVerifyInstallations
            | Self::NoWelcomesToSend => false,
        }
    }
}
