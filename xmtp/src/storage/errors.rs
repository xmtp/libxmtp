use thiserror::Error;

#[derive(Debug, Error)]
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
    #[error("unknown storage error: {0}")]
    Unknown(String),
}
