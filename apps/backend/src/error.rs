//! Typed backend storage/admission failures translated by the service boundary.
//! Test builds additionally expose partition-provisioning outcomes for disposable
//! benchmark retries; those variants are not part of the production error surface.

use xmtp_mls_validation::ValidationError;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[cfg(test)]
    #[error("benchmark publish requires sequence partitions for {0:?}")]
    MissingEnvelopePartitions(Vec<i64>),
    #[cfg(test)]
    #[error("partitioned benchmark publish exhausted its deadline or retry budget")]
    PartitionPublishDeadline,
    #[error(transparent)]
    AttachmentSigning(#[from] xmtp_attachments_server::SignError),
    #[error("recipient is not registered")]
    PushRecipientMissing,
    #[error("recipient secret is not valid")]
    PushSecretInvalid,
    #[error("recipient topic limit reached")]
    PushTopicLimit,
    #[error("recipient expiry cannot be represented")]
    PushExpiryOverflow,
    #[error("identity history changed during validation")]
    StaleHistory,
    #[error("envelope admission failed")]
    Admission { index: usize, error: AdmissionError },
    #[error("database operation failed")]
    Database(#[from] sqlx::Error),
    #[error("database migration failed")]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error("startup allocation-boundary read exceeded {timeout_ms}ms")]
    StartupBoundaryTimeout { timeout_ms: u64 },
    #[error("storage invariant failed: {0}")]
    Invariant(&'static str),
}

#[derive(Clone, Debug)]
pub enum AdmissionError {
    Validation(std::sync::Arc<ValidationError>),
    TooLarge(&'static str),
    InvalidIdentity(&'static str),
    /// A claim that every validator accepts and the backend's handle policy refuses.
    Handle(crate::handles::HandleRejection),
}

impl From<ValidationError> for AdmissionError {
    fn from(error: ValidationError) -> Self {
        Self::Validation(std::sync::Arc::new(error))
    }
}
