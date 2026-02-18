use diesel::result::DatabaseErrorKind;
use thiserror::Error;
use xmtp_common::ErrorCode;

use crate::group_intent::GroupIntentError;

use super::{
    refresh_state::EntityKind,
    sql_key_store::{self, SqlKeyStoreError},
};
use xmtp_common::{BoxDynError, RetryableError, retryable};
use xmtp_proto::types::{Cursor, InstallationId};

pub struct Mls;

#[derive(Debug, Error, ErrorCode)]
pub enum StorageError {
    /// Diesel connection error.
    ///
    /// Failed to connect to SQLite. Retryable.
    #[error(transparent)]
    DieselConnect(#[from] diesel::ConnectionError),
    /// Diesel result error.
    ///
    /// Database query returned an error. May be retryable.
    #[error(transparent)]
    DieselResult(#[from] diesel::result::Error),
    /// Migration error.
    ///
    /// Database migration failed. Not retryable.
    #[error("Error migrating database {0}")]
    MigrationError(#[from] BoxDynError),
    /// Not found.
    ///
    /// Requested record does not exist. Not retryable.
    #[error(transparent)]
    NotFound(#[from] NotFound),
    /// Duplicate item.
    ///
    /// Attempted to insert a duplicate record. Not retryable.
    #[error(transparent)]
    Duplicate(DuplicateItem),
    /// OpenMLS storage error.
    ///
    /// OpenMLS key store operation failed. Not retryable.
    #[error(transparent)]
    OpenMlsStorage(#[from] SqlKeyStoreError),
    /// Intentional rollback.
    ///
    /// Transaction was intentionally rolled back. Not retryable.
    #[error("Transaction was intentionally rolled back")]
    IntentionalRollback,
    /// DB deserialization failed.
    ///
    /// Failed to deserialize data from database. Not retryable.
    #[error("failed to deserialize from db")]
    DbDeserialize,
    /// DB serialization failed.
    ///
    /// Failed to serialize data for database. Not retryable.
    #[error("failed to serialize for db")]
    DbSerialize,
    /// Builder error.
    ///
    /// Required fields missing from stored type. Not retryable.
    #[error("required fields missing from stored db type {0}")]
    Builder(#[from] derive_builder::UninitializedFieldError),
    /// Platform storage error.
    ///
    /// Platform-specific storage error. May be retryable.
    #[error(transparent)]
    Platform(#[from] crate::database::PlatformStorageError),
    /// Protobuf decode error.
    ///
    /// Failed to decode protobuf from database. Not retryable.
    #[error("decoding from database failed {}", _0)]
    Prost(#[from] prost::DecodeError),
    /// Conversion error.
    ///
    /// Proto conversion failed. Not retryable.
    #[error(transparent)]
    Conversion(#[from] xmtp_proto::ConversionError),
    /// Connection error.
    ///
    /// Database connection error. Retryable.
    #[error(transparent)]
    Connection(#[from] crate::ConnectionError),
    /// Invalid HMAC length.
    ///
    /// HMAC key must be 42 bytes. Not retryable.
    #[error("HMAC key must be 42 bytes")]
    InvalidHmacLength,
    /// Group intent error.
    ///
    /// Group intent processing failed. May be retryable.
    #[error(transparent)]
    GroupIntent(#[from] GroupIntentError),
}

impl From<std::convert::Infallible> for StorageError {
    fn from(_: std::convert::Infallible) -> StorageError {
        // infallible can never fail/occur
        unreachable!("Infallible conversion should never fail.")
    }
}

impl StorageError {
    // release conn is a noop in wasm
    #[cfg(target_arch = "wasm32")]
    pub fn db_needs_connection(&self) -> bool {
        false
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn db_needs_connection(&self) -> bool {
        use StorageError::*;
        matches!(
            self,
            Platform(crate::PlatformStorageError::PoolNeedsConnection)
                | Connection(crate::ConnectionError::Platform(
                    crate::PlatformStorageError::PoolNeedsConnection,
                ))
        )
    }
}

#[derive(Error, Debug, ErrorCode)]
// Monolithic enum for all things lost
pub enum NotFound {
    /// Group with welcome ID not found.
    ///
    /// No group matches the welcome ID. Not retryable.
    #[error("group with welcome id {0} not found")]
    GroupByWelcome(Cursor),
    /// Group with ID not found.
    ///
    /// Group does not exist in local DB. Not retryable.
    #[error("group with id {id} not found", id = hex::encode(_0))]
    GroupById(Vec<u8>),
    /// Installation time for group not found.
    ///
    /// Missing installation timestamp. Not retryable.
    #[error("installation time for group {id}", id = hex::encode(_0))]
    InstallationTimeForGroup(Vec<u8>),
    /// Inbox ID for address not found.
    ///
    /// Address has no associated inbox. Not retryable.
    #[error("inbox id for address {0} not found")]
    InboxIdForAddress(String),
    /// Message ID not found.
    ///
    /// Message does not exist in local DB. Not retryable.
    #[error("message id {id} not found", id = hex::encode(_0))]
    MessageById(Vec<u8>),
    /// DM by inbox ID not found.
    ///
    /// No DM conversation with this inbox. Not retryable.
    #[error("dm by dm_target_inbox_id {0} not found")]
    DmByInbox(String),
    /// Intent for ToPublish not found.
    ///
    /// Intent with specified ID not in expected state. Not retryable.
    #[error("intent with id {0} for state Publish from ToPublish not found")]
    IntentForToPublish(i32),
    /// Intent for Published not found.
    ///
    /// Intent with specified ID not in expected state. Not retryable.
    #[error("intent with id {0} for state ToPublish from Published not found")]
    IntentForPublish(i32),
    /// Intent for Committed not found.
    ///
    /// Intent with specified ID not in expected state. Not retryable.
    #[error("intent with id {0} for state Committed from Published not found")]
    IntentForCommitted(i32),
    /// Intent by ID not found.
    ///
    /// Intent does not exist. Not retryable.
    #[error("Intent with id {0} not found")]
    IntentById(i32),
    /// Refresh state not found.
    ///
    /// No refresh state matching criteria. Not retryable.
    #[error("refresh state with id {id} of kind {1} originating from node {2} not found", id = hex::encode(_0))]
    RefreshStateByIdKindAndOriginator(Vec<u8>, EntityKind, i32),
    /// Cipher salt not found.
    ///
    /// Database encryption salt missing. Not retryable.
    #[error("Cipher salt for db at [`{0}`] not found")]
    CipherSalt(String),
    /// Sync group not found.
    ///
    /// No sync group for this installation. Not retryable.
    #[error("Sync Group for installation {0} not found")]
    SyncGroup(InstallationId),
    /// Key package reference not found.
    ///
    /// Key package handle not in store. Not retryable.
    #[error("Key Package Reference {handle} not found", handle = hex::encode(_0))]
    KeyPackageReference(Vec<u8>),
    /// MLS group not found.
    ///
    /// OpenMLS group not in local state. Not retryable.
    #[error("MLS Group Not Found")]
    MlsGroup,
    /// Post-quantum private key not found.
    ///
    /// PQ key pair not in store. Not retryable.
    #[error("Post Quantum Key Pair not found")]
    PostQuantumPrivateKey,
    /// Key package not found.
    ///
    /// Key package not in store. Not retryable.
    #[error("Key Package {kp} not found", kp = hex::encode(_0))]
    KeyPackage(Vec<u8>),
}

#[derive(Error, Debug, ErrorCode)]
#[error_code(internal)]
pub enum DuplicateItem {
    /// Duplicate welcome ID.
    ///
    /// Welcome ID already exists. Not retryable.
    #[error("the welcome id {0:?} already exists")]
    WelcomeId(Option<Cursor>),
    /// Duplicate commit log public key.
    ///
    /// Commit log public key for group already exists. Not retryable.
    #[error("the commit log public key for group id {id} already exists", id = hex::encode(_0))]
    CommitLogPublicKey(Vec<u8>),
}

impl RetryableError for DuplicateItem {
    fn is_retryable(&self) -> bool {
        use DuplicateItem::*;
        match self {
            WelcomeId(_) => false,
            CommitLogPublicKey(_) => false,
        }
    }
}

impl RetryableError<Mls> for diesel::result::Error {
    fn is_retryable(&self) -> bool {
        use DatabaseErrorKind::*;
        use diesel::result::Error::*;

        match self {
            DatabaseError(UniqueViolation, _) => false,
            DatabaseError(CheckViolation, _) => false,
            DatabaseError(NotNullViolation, _) => false,
            // TODO: Figure out the full list of non-retryable errors.
            // The diesel code has a comment that "this type is not meant to be exhaustively matched"
            // so best is probably to return true here and map known errors to something else
            // that is not retryable.
            _ => true,
        }
    }
}

impl RetryableError for StorageError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::DieselConnect(_) => true,
            Self::DieselResult(result) => retryable!(result),
            Self::Duplicate(d) => retryable!(d),
            Self::OpenMlsStorage(storage) => retryable!(storage),
            Self::Platform(p) => retryable!(p),
            Self::Connection(e) => retryable!(e),
            Self::GroupIntent(e) => retryable!(e),
            Self::MigrationError(_)
            | Self::Conversion(_)
            | Self::NotFound(_)
            | Self::IntentionalRollback
            | Self::DbDeserialize
            | Self::DbSerialize
            | Self::Builder(_)
            | Self::InvalidHmacLength
            | Self::Prost(_) => false,
        }
    }
}

impl RetryableError for NotFound {
    fn is_retryable(&self) -> bool {
        true
    }
}

// OpenMLS KeyStore errors
impl RetryableError<Mls> for openmls::group::AddMembersError<sql_key_store::SqlKeyStoreError> {
    fn is_retryable(&self) -> bool {
        match self {
            Self::CreateCommitError(commit) => retryable!(commit),
            Self::CommitBuilderStageError(commit_builder_stage) => retryable!(commit_builder_stage),
            Self::StorageError(storage) => retryable!(storage),
            Self::GroupStateError(group_state) => retryable!(group_state),
            _ => false,
        }
    }
}

impl RetryableError<Mls> for openmls::group::CreateCommitError {
    fn is_retryable(&self) -> bool {
        false
    }
}

impl RetryableError<Mls>
    for openmls::treesync::LeafNodeUpdateError<sql_key_store::SqlKeyStoreError>
{
    fn is_retryable(&self) -> bool {
        match self {
            Self::Storage(storage) => retryable!(storage),
            _ => false,
        }
    }
}

impl RetryableError<Mls> for openmls::key_packages::errors::KeyPackageNewError {
    fn is_retryable(&self) -> bool {
        matches!(self, Self::StorageError)
    }
}

impl RetryableError<Mls> for openmls::group::RemoveMembersError<sql_key_store::SqlKeyStoreError> {
    fn is_retryable(&self) -> bool {
        match self {
            Self::CreateCommitError(commit) => retryable!(commit),
            Self::CommitBuilderStageError(commit_builder_stage) => retryable!(commit_builder_stage),
            Self::GroupStateError(group_state) => retryable!(group_state),
            Self::StorageError(storage) => retryable!(storage),
            _ => false,
        }
    }
}

impl RetryableError<Mls> for openmls::group::NewGroupError<sql_key_store::SqlKeyStoreError> {
    fn is_retryable(&self) -> bool {
        match self {
            Self::StorageError(storage) => retryable!(storage),
            _ => false,
        }
    }
}

impl RetryableError<Mls>
    for openmls::group::UpdateGroupMembershipError<sql_key_store::SqlKeyStoreError>
{
    fn is_retryable(&self) -> bool {
        match self {
            Self::CreateCommitError(create_commit) => retryable!(create_commit),
            Self::GroupStateError(group_state) => retryable!(group_state),
            Self::StorageError(storage) => retryable!(storage),
            Self::CommitBuilderError(commit_builder) => retryable!(commit_builder),
        }
    }
}

impl RetryableError<Mls> for openmls::prelude::MlsGroupStateError {
    fn is_retryable(&self) -> bool {
        false
    }
}

impl RetryableError<Mls>
    for openmls::prelude::CreateGroupContextExtProposalError<sql_key_store::SqlKeyStoreError>
{
    fn is_retryable(&self) -> bool {
        match self {
            Self::CreateCommitError(create_commit) => retryable!(create_commit),
            Self::StorageError(storage) => retryable!(storage),
            _ => false,
        }
    }
}

impl RetryableError<Mls> for openmls::group::SelfUpdateError<sql_key_store::SqlKeyStoreError> {
    fn is_retryable(&self) -> bool {
        match self {
            Self::CreateCommitError(commit) => retryable!(commit),
            Self::CommitBuilderStageError(commit_builder_stage) => retryable!(commit_builder_stage),
            Self::GroupStateError(group_state) => retryable!(group_state),
            Self::StorageError(storage) => retryable!(storage),
            _ => false,
        }
    }
}

impl RetryableError<Mls>
    for openmls::group::CommitBuilderStageError<sql_key_store::SqlKeyStoreError>
{
    fn is_retryable(&self) -> bool {
        match self {
            Self::KeyStoreError(storage) => retryable!(storage),
            Self::LibraryError(_) => false,
        }
    }
}

impl RetryableError<Mls>
    for openmls::prelude::CreationFromExternalError<sql_key_store::SqlKeyStoreError>
{
    fn is_retryable(&self) -> bool {
        match self {
            Self::WriteToStorageError(storage) => retryable!(storage),
            _ => false,
        }
    }
}

impl RetryableError<Mls> for openmls::prelude::WelcomeError<sql_key_store::SqlKeyStoreError> {
    fn is_retryable(&self) -> bool {
        match self {
            Self::PublicGroupError(creation_err) => retryable!(creation_err),
            Self::StorageError(storage) => retryable!(storage),
            _ => false,
        }
    }
}

impl RetryableError<Mls> for openmls::group::MergeCommitError<sql_key_store::SqlKeyStoreError> {
    fn is_retryable(&self) -> bool {
        match self {
            Self::StorageError(storage) => retryable!(storage),
            Self::LibraryError(_) => false,
        }
    }
}

impl RetryableError<Mls>
    for openmls::group::MergePendingCommitError<sql_key_store::SqlKeyStoreError>
{
    fn is_retryable(&self) -> bool {
        match self {
            Self::MlsGroupStateError(err) => retryable!(err),
            Self::MergeCommitError(err) => retryable!(err),
        }
    }
}

impl RetryableError<Mls>
    for openmls::prelude::ProcessMessageError<sql_key_store::SqlKeyStoreError>
{
    fn is_retryable(&self) -> bool {
        match self {
            Self::GroupStateError(err) => retryable!(err),
            _ => false,
        }
    }
}
