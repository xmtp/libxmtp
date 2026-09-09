use crate::{
    api::{PublishError, publish_error::Reason},
    error::{AdmissionError, Error},
};
use prost::Message;
use tonic::{Code, Status};
use xmtp_common::RetryableError;

impl From<Error> for Status {
    /// Map typed backend failures to stable gRPC status classes.
    ///
    /// Admission errors retain their input index and reason. Database timeout
    /// and availability failures stay distinguishable from storage invariants.
    fn from(error: Error) -> Self {
        use crate::telemetry::{self, DbErrorKind};
        let kind = match &error {
            Error::StaleHistory | Error::Admission { .. } => None,
            Error::Database(sqlx::Error::Database(db))
                if matches!(db.code().as_deref(), Some("57014" | "25P04")) =>
            {
                Some(DbErrorKind::Timeout)
            }
            Error::Database(sqlx::Error::PoolTimedOut) => Some(DbErrorKind::Timeout),
            Error::Invariant(_) => Some(DbErrorKind::Invariant),
            Error::Database(sqlx::Error::Database(db))
                if matches!(db.code().as_deref(), Some("23505" | "23514" | "22003")) =>
            {
                Some(DbErrorKind::Invariant)
            }
            Error::Database(sqlx::Error::Io(_) | sqlx::Error::Tls(_) | sqlx::Error::PoolClosed) => {
                Some(DbErrorKind::Connection)
            }
            Error::Database(sqlx::Error::Database(db))
                if db
                    .code()
                    .is_some_and(|code| code.starts_with("08") || code.starts_with("57P")) =>
            {
                Some(DbErrorKind::Connection)
            }
            Error::Database(_) | Error::Migration(_) => Some(DbErrorKind::Other),
        };
        if let Some(kind) = kind {
            telemetry::db_error(kind);
        }
        match &error {
            Error::StaleHistory => Self::aborted("identity history changed during validation"),
            Error::Admission { index, error } => error.status(*index),
            Error::Database(sqlx::Error::Database(db))
                if matches!(db.code().as_deref(), Some("57014" | "25P04")) =>
            {
                Self::deadline_exceeded("database operation timed out")
            }
            Error::Database(sqlx::Error::Database(db))
                if matches!(db.code().as_deref(), Some("23505" | "23514" | "22003")) =>
            {
                tracing::error!("publish storage invariant failed");
                Self::internal("storage invariant failed")
            }
            Error::Database(_) | Error::Migration(_) => {
                Self::unavailable("database operation failed")
            }
            _ => {
                tracing::error!(error = %error, "stored data invariant failed");
                Self::internal("stored data invariant failed")
            }
        }
    }
}

/// Build an `INVALID_ARGUMENT` status with a structured publish error detail.
///
/// `index` identifies the failing envelope when the error is envelope-specific.
/// Request-level errors pass `None`; the detail still uses the same wire type.
pub fn publish_invalid(index: Option<usize>, reason: Reason, message: impl Into<String>) -> Status {
    crate::telemetry::publish_rejected(reason);
    let message = message.into();
    let detail = PublishError {
        index: index.map(|index| index as u32),
        reason: reason.into(),
        message: message.clone(),
    };
    let status = tonic_types::pb::Status {
        code: Code::InvalidArgument as i32,
        message: message.clone(),
        details: vec![prost_types::Any {
            type_url: "type.googleapis.com/xmtp.backend.v1.PublishError".into(),
            value: detail.encode_to_vec(),
        }],
    };
    Status::with_details(
        Code::InvalidArgument,
        message,
        status.encode_to_vec().into(),
    )
}

impl AdmissionError {
    /// Convert an admission error for one input into its transport status.
    ///
    /// Retryable verifier failures become `UNAVAILABLE`; validation and size
    /// failures remain indexed `INVALID_ARGUMENT` responses.
    fn status(&self, index: usize) -> Status {
        match self {
            Self::Validation(error) if error.is_retryable() => {
                Status::unavailable("signature verifier unavailable")
            }
            Self::Validation(error) => {
                publish_invalid(Some(index), error.reason(), "envelope validation failed")
            }
            Self::TooLarge(message) => publish_invalid(Some(index), Reason::TooLarge, *message),
            Self::InvalidIdentity(message) => {
                publish_invalid(Some(index), Reason::InvalidIdentityUpdate, *message)
            }
        }
    }
}
