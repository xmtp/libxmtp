use std::sync::PoisonError;

use thiserror::Error;

use crate::{retry::RetryableError, retryable};

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
}

impl<T> From<PoisonError<T>> for StorageError {
    fn from(_: PoisonError<T>) -> Self {
        StorageError::Lock("Lock poisoned".into())
    }
}

impl RetryableError for diesel::result::Error {
    fn is_retryable(&self) -> bool {
        log::error!("retrying diesel error {:?}", self);
        format!("{:?}", self).contains("database is locked")
    }
}

impl RetryableError for StorageError {
    fn is_retryable(&self) -> bool {
        log::info!("retrying storage error {:?}", self);
        match self {
            Self::DieselConnect(connection) => {
                matches!(connection, diesel::ConnectionError::BadConnection(_))
            }
            Self::DieselResult(result) => {
                log::warn!("got a disel result error");
                format!("{:?}", result).contains("database is locked")
            }
            Self::Pool(_) => true,
            _ => false,
        }
    }
}

// OpenMLS KeyStore errors
impl RetryableError for openmls::group::AddMembersError<StorageError> {
    fn is_retryable(&self) -> bool {
        match self {
            Self::CreateCommitError(commit) => retryable!(commit),
            _ => false,
        }
    }
}

impl RetryableError for openmls::group::CreateCommitError<StorageError> {
    fn is_retryable(&self) -> bool {
        match self {
            Self::KeyStoreError(storage) => retryable!(storage),
            Self::KeyPackageGenerationError(generation) => retryable!(generation),
            _ => false,
        }
    }
}

impl RetryableError for openmls::key_packages::errors::KeyPackageNewError {
    fn is_retryable(&self) -> bool {
        matches!(self, Self::StorageError)
    }
}

impl RetryableError for openmls::group::RemoveMembersError<StorageError> {
    fn is_retryable(&self) -> bool {
        match self {
            Self::CreateCommitError(commit) => retryable!(commit),
            _ => false,
        }
    }
}

impl RetryableError for openmls::group::NewGroupError<StorageError> {
    fn is_retryable(&self) -> bool {
        match self {
            Self::StorageError(storage) => retryable!(storage),
            _ => false,
        }
    }
}

impl RetryableError for openmls::group::UpdateGroupMembershipError<StorageError> {
    fn is_retryable(&self) -> bool {
        match self {
            Self::StorageError(storage) => retryable!(storage),
            _ => false,
        }
    }
}

impl RetryableError for openmls::group::SelfUpdateError<StorageError> {
    fn is_retryable(&self) -> bool {
        match self {
            Self::CreateCommitError(commit) => retryable!(commit),
            Self::StorageError(storage) => retryable!(storage),
            _ => false,
        }
    }
}

impl RetryableError for openmls::group::WelcomeError<StorageError> {
    fn is_retryable(&self) -> bool {
        match self {
            Self::StorageError(storage) => retryable!(storage),
            _ => false,
        }
    }
}
