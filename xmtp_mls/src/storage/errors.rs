use thiserror::Error;

use crate::{retry::RetryableError, retryable};

#[derive(Debug, Error, PartialEq)]
pub enum StorageError {
    #[error("Diesel connection error")]
    DieselConnect(#[from] diesel::ConnectionError),
    #[error("Diesel result error: {0}")]
    DieselResult(#[from] diesel::result::Error),
    #[error("Pool error: {0}")]
    Pool(String),
    #[error("Either incorrect encryptionkey or file is not a db: {0}")]
    DbInit(String),
    #[error("Store Error")]
    Store(String),
    #[error("serialization error")]
    Serialization,
    #[error("deserialization error")]
    Deserialization,
    #[error("not found")]
    NotFound,
}

impl RetryableError for StorageError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::DieselConnect(connection) => {
                matches!(connection, diesel::ConnectionError::BadConnection(_))
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

impl RetryableError for openmls::key_packages::errors::KeyPackageNewError<StorageError> {
    fn is_retryable(&self) -> bool {
        match self {
            Self::KeyStoreError(storage) => retryable!(storage),
            _ => false,
        }
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
            Self::KeyStoreError(storage) => retryable!(storage),
            _ => false,
        }
    }
}

impl RetryableError for openmls::group::SelfUpdateError<StorageError> {
    fn is_retryable(&self) -> bool {
        match self {
            Self::CreateCommitError(commit) => retryable!(commit),
            Self::KeyStoreError => true,
            _ => false,
        }
    }
}

impl RetryableError for openmls::group::WelcomeError<StorageError> {
    fn is_retryable(&self) -> bool {
        match self {
            Self::KeyStoreError(storage) => retryable!(storage),
            _ => false,
        }
    }
}
