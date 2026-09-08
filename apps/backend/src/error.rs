use xmtp_mls_validation::ValidationError;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("identity history changed during validation")]
    StaleHistory,
    #[error("envelope admission failed")]
    Admission { index: usize, error: AdmissionError },
    #[error("database operation failed")]
    Database(#[from] sqlx::Error),
    #[error("database migration failed")]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error("storage invariant failed: {0}")]
    Invariant(&'static str),
}

#[derive(Clone, Debug)]
pub enum AdmissionError {
    Validation(std::sync::Arc<ValidationError>),
    TooLarge(&'static str),
    InvalidIdentity(&'static str),
}

impl From<ValidationError> for AdmissionError {
    fn from(error: ValidationError) -> Self {
        Self::Validation(std::sync::Arc::new(error))
    }
}
