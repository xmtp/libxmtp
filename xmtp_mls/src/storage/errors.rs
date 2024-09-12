use std::sync::PoisonError;

use diesel::result::DatabaseErrorKind;
use thiserror::Error;

use crate::{groups::intents::IntentError, retry::RetryableError, retryable};

use super::sql_key_store;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("Diesel connection error")]
    DieselConnect(#[from] diesel::ConnectionError),
    #[error("Diesel result error: {0}")]
    DieselResult(#[from] diesel::result::Error),
    #[error("Pool error: {0}")]
    Pool(#[from] diesel::r2d2::PoolError),
    #[error("Error with connection to Sqlite {0}")]
    DbConnection(#[from] diesel::r2d2::Error),
    #[error("incorrect encryptionkey or file is not a database: {0}")]
    DbInit(String),
    #[error("Error migrating database {0}")]
    MigrationError(#[from] Box<dyn std::error::Error + Send + Sync>),
    #[error("serialization error")]
    Serialization(String),
    #[error("deserialization error")]
    Deserialization(String),
    #[error("{0} not found")]
    NotFound(String),
    #[error("lock")]
    Lock(String),
    #[error("Pool needs to  reconnect before use")]
    PoolNeedsConnection,
    #[error("Conflict")]
    Conflict(String),
    #[error(transparent)]
    Intent(#[from] IntentError),
    #[error("The SQLCipher Sqlite extension is not present, but an encryption key is given")]
    SqlCipherNotLoaded,
    #[error("PRAGMA key or salt has incorrect value")]
    SqlCipherKeyIncorrect,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    FromHex(#[from] hex::FromHexError),
}

impl<T> From<PoisonError<T>> for StorageError {
    fn from(_: PoisonError<T>) -> Self {
        StorageError::Lock("Lock poisoned".into())
    }
}

impl RetryableError for diesel::result::Error {
    fn is_retryable(&self) -> bool {
        match self {
            Self::DatabaseError(DatabaseErrorKind::UniqueViolation, _) => false,
            Self::DatabaseError(DatabaseErrorKind::CheckViolation, _) => false,
            Self::DatabaseError(DatabaseErrorKind::NotNullViolation, _) => false,
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
            Self::Pool(_) => true,
            Self::Lock(_) => true,
            Self::SqlCipherNotLoaded => true,
            Self::PoolNeedsConnection => true,
            Self::SqlCipherKeyIncorrect => false,
            _ => false,
        }
    }
}

// OpenMLS KeyStore errors
impl RetryableError for openmls::group::AddMembersError<sql_key_store::SqlKeyStoreError> {
    fn is_retryable(&self) -> bool {
        match self {
            Self::CreateCommitError(commit) => retryable!(commit),
            Self::StorageError(storage) => retryable!(storage),
            Self::GroupStateError(group_state) => retryable!(group_state),
            _ => false,
        }
    }
}

impl RetryableError for openmls::group::CreateCommitError<sql_key_store::SqlKeyStoreError> {
    fn is_retryable(&self) -> bool {
        match self {
            Self::KeyStoreError(storage) => retryable!(storage),
            Self::LeafNodeUpdateError(leaf_node_update) => retryable!(leaf_node_update),
            _ => false,
        }
    }
}

impl RetryableError for openmls::treesync::LeafNodeUpdateError<sql_key_store::SqlKeyStoreError> {
    fn is_retryable(&self) -> bool {
        match self {
            Self::Storage(storage) => retryable!(storage),
            _ => false,
        }
    }
}

impl RetryableError for openmls::key_packages::errors::KeyPackageNewError {
    fn is_retryable(&self) -> bool {
        matches!(self, Self::StorageError)
    }
}

impl RetryableError for openmls::group::RemoveMembersError<sql_key_store::SqlKeyStoreError> {
    fn is_retryable(&self) -> bool {
        match self {
            Self::CreateCommitError(commit) => retryable!(commit),
            Self::GroupStateError(group_state) => retryable!(group_state),
            Self::StorageError(storage) => retryable!(storage),
            _ => false,
        }
    }
}

impl RetryableError for openmls::group::NewGroupError<sql_key_store::SqlKeyStoreError> {
    fn is_retryable(&self) -> bool {
        match self {
            Self::StorageError(storage) => retryable!(storage),
            _ => false,
        }
    }
}

impl RetryableError
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

impl RetryableError for openmls::prelude::MlsGroupStateError {
    fn is_retryable(&self) -> bool {
        false
    }
}

impl RetryableError
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

impl RetryableError for openmls::group::SelfUpdateError<sql_key_store::SqlKeyStoreError> {
    fn is_retryable(&self) -> bool {
        match self {
            Self::CreateCommitError(commit) => retryable!(commit),
            Self::GroupStateError(group_state) => retryable!(group_state),
            Self::StorageError(storage) => retryable!(storage),
            _ => false,
        }
    }
}

impl RetryableError
    for openmls::prelude::CreationFromExternalError<sql_key_store::SqlKeyStoreError>
{
    fn is_retryable(&self) -> bool {
        match self {
            Self::WriteToStorageError(storage) => retryable!(storage),
            _ => false,
        }
    }
}

impl RetryableError for openmls::prelude::WelcomeError<sql_key_store::SqlKeyStoreError> {
    fn is_retryable(&self) -> bool {
        match self {
            Self::PublicGroupError(creation_err) => retryable!(creation_err),
            Self::StorageError(storage) => retryable!(storage),
            _ => false,
        }
    }
}

impl RetryableError for openmls::group::MergeCommitError<sql_key_store::SqlKeyStoreError> {
    fn is_retryable(&self) -> bool {
        match self {
            Self::StorageError(storage) => retryable!(storage),
            _ => false,
        }
    }
}

impl RetryableError for openmls::group::MergePendingCommitError<sql_key_store::SqlKeyStoreError> {
    fn is_retryable(&self) -> bool {
        match self {
            Self::MlsGroupStateError(err) => retryable!(err),
            Self::MergeCommitError(err) => retryable!(err),
        }
    }
}

impl RetryableError for openmls::prelude::ProcessMessageError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::GroupStateError(err) => retryable!(err),
            _ => false,
        }
    }
}
