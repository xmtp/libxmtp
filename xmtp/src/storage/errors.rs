use thiserror::Error;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("Diesel connection error")]
    DieselConnectError(#[from] diesel::ConnectionError),
    #[error("Diesel result error")]
    DieselResultError(#[from] diesel::result::Error),
    #[error("Pool error {0}")]
    PoolError(String),
    #[error("Either incorrect encryptionkey or file is not a db {0}")]
    DbInitError(String),
    #[error(transparent)]
    ImplementationError(#[from] anyhow::Error),
    #[error("unknown storage error")]
    Unknown,
}
