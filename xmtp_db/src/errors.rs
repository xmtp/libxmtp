use diesel::result::DatabaseErrorKind;
use thiserror::Error;

use super::{
    refresh_state::EntityKind,
    sql_key_store::{self, SqlKeyStoreError},
};
use xmtp_common::{RetryableError, retryable, types::InstallationId};

pub struct Mls;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error(transparent)]
    DieselConnect(#[from] diesel::ConnectionError),
    #[error(transparent)]
    DieselResult(#[from] diesel::result::Error),
    #[error("Error migrating database {0}")]
    MigrationError(#[from] Box<dyn std::error::Error + Send + Sync>),
    #[error(transparent)]
    Conversion(#[from] xmtp_proto::ConversionError),
    #[error(transparent)]
    NotFound(#[from] NotFound),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    FromHex(#[from] hex::FromHexError),
    #[error(transparent)]
    Duplicate(DuplicateItem),
    #[error(transparent)]
    OpenMlsStorage(#[from] SqlKeyStoreError),
    #[error("generic:{0}")]
    Generic(String),
    #[error("Transaction was intentionally rolled back")]
    IntentionalRollback,
    #[error("failed to deserialize from db")]
    DbDeserialize,
    #[error("failed to serialize for db")]
    DbSerialize,
    #[error(transparent)]
    MissingRequired(#[from] MissingRequired),
    #[error("required fields missing from stored db type {0}")]
    Builder(#[from] derive_builder::UninitializedFieldError),
    #[error(transparent)]
    Platform(#[from] crate::database::PlatformStorageError),
    #[error("decoding from database failed {}", _0)]
    Prost(#[from] prost::DecodeError),
    #[error(transparent)]
    Connection(#[from] crate::ConnectionError),
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
        matches!(
            self,
            Self::Platform(crate::database::native::PlatformStorageError::PoolNeedsConnection)
        )
    }
}

#[derive(Error, Debug)]
// Monolithic enum for all things lost
pub enum NotFound {
    #[error("group with welcome id {0} not found")]
    GroupByWelcome(i64),
    #[error("group with id {id} not found", id = hex::encode(_0))]
    GroupById(Vec<u8>),
    #[error("installation time for group {id}", id = hex::encode(_0))]
    InstallationTimeForGroup(Vec<u8>),
    #[error("inbox id for address {0} not found")]
    InboxIdForAddress(String),
    #[error("message id {id} not found", id = hex::encode(_0))]
    MessageById(Vec<u8>),
    #[error("dm by dm_target_inbox_id {0} not found")]
    DmByInbox(String),
    #[error("intent with id {0} for state Publish from ToPublish not found")]
    IntentForToPublish(i32),
    #[error("intent with id {0} for state ToPublish from Published not found")]
    IntentForPublish(i32),
    #[error("intent with id {0} for state Committed from Published not found")]
    IntentForCommitted(i32),
    #[error("Intent with id {0} not found")]
    IntentById(i32),
    #[error("refresh state with id {id} and kind {1} not found", id = hex::encode(_0))]
    RefreshStateByIdAndKind(Vec<u8>, EntityKind),
    #[error("Cipher salt for db at [`{0}`] not found")]
    CipherSalt(String),
    #[error("Sync Group for installation {0} not found")]
    SyncGroup(InstallationId),
    #[error("MLS Group Not Found")]
    MlsGroup,
}

#[derive(Error, Debug)]
pub enum MissingRequired {
    #[error("Identifier kind is required when entity type is Identity")]
    IdentifierKind,
}

#[derive(Error, Debug)]
pub enum DuplicateItem {
    #[error("the welcome id {0:?} already exists")]
    WelcomeId(Option<i64>),
}

impl RetryableError for DuplicateItem {
    fn is_retryable(&self) -> bool {
        use DuplicateItem::*;
        match self {
            WelcomeId(_) => false,
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
            Self::Io(_) => true,
            Self::OpenMlsStorage(storage) => retryable!(storage),
            Self::Platform(p) => retryable!(p),
            Self::Connection(e) => retryable!(e),
            Self::MigrationError(_)
                | Self::Conversion(_)
                | Self::NotFound(_)
                | Self::FromHex(_)
                | Self::Generic(_) // TODO Audit generic errors and turn into variants
                | Self::IntentionalRollback
                | Self::DbDeserialize
                | Self::DbSerialize
                | Self::MissingRequired(_)
                | Self::Builder(_)
                | Self::Prost(_)
            => false
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
            Self::StorageError(storage) => retryable!(storage),
            Self::GroupStateError(group_state) => retryable!(group_state),
            _ => false,
        }
    }
}

impl RetryableError<Mls> for openmls::group::CreateCommitError<sql_key_store::SqlKeyStoreError> {
    fn is_retryable(&self) -> bool {
        match self {
            Self::KeyStoreError(storage) => retryable!(storage),
            Self::LeafNodeUpdateError(leaf_node_update) => retryable!(leaf_node_update),
            _ => false,
        }
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
            _ => false,
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
            Self::GroupStateError(group_state) => retryable!(group_state),
            Self::StorageError(storage) => retryable!(storage),
            _ => false,
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
            _ => false,
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

impl RetryableError<Mls> for openmls::prelude::ProcessMessageError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::GroupStateError(err) => retryable!(err),
            _ => false,
        }
    }
}
