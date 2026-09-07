use prost::Message;
use tonic::{Code, Status};
use xmtp_common::RetryableError;
use xmtp_mls_validation::ValidationError;

use crate::api::{PublishError, publish_error::Reason};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database operation failed")]
    Database(#[from] sqlx::Error),
    #[error("database migration failed")]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error("stored envelope is invalid")]
    Decode(#[from] prost::DecodeError),
    #[error("storage invariant failed: {0}")]
    Invariant(&'static str),
}

impl From<Error> for Status {
    fn from(error: Error) -> Self {
        match &error {
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

pub fn publish_invalid(index: Option<usize>, reason: Reason, message: impl Into<String>) -> Status {
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

pub fn validation_status(index: usize, error: ValidationError) -> Status {
    if error.is_retryable() {
        Status::unavailable("signature verifier unavailable")
    } else {
        publish_invalid(Some(index), error.reason(), "envelope validation failed")
    }
}
