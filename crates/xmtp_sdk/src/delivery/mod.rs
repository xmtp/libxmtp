mod connection_state;
mod conversation_reader;
mod message_reader;

pub use connection_state::ConnectionState;
pub use conversation_reader::ConversationReader;
pub use message_reader::MessageReader;

#[cfg(test)]
pub(crate) use message_reader::{HandoffGate, selection_changed};

use crate::{ErrorCategory, ErrorDetails, XmtpError};
use std::error::Error;
use tokio_util::sync::CancellationToken;
use xmtp_db::{StorageError, stream_storage::StreamStorageError};
use xmtp_mls::{client::ClientError, subscriptions::local_delivery::LocalDeliveryError};
use xmtp_proto::api::{ApiClientError, AuthError};

/// Stop a detached read when its caller leaves, including on cancellation.
pub(super) struct CancelReadOnDrop(pub CancellationToken);

impl Drop for CancelReadOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

fn details(code: &str, category: ErrorCategory, retryable: bool, message: String) -> ErrorDetails {
    ErrorDetails {
        code: code.into(),
        category,
        retryable,
        message,
    }
}

pub(crate) fn delivery_error(error: LocalDeliveryError) -> XmtpError {
    let mut cause = &error;
    while let LocalDeliveryError::SessionFailure(inner) = cause {
        cause = inner.as_ref();
    }
    if matches!(cause, LocalDeliveryError::NetworkRecoveryExhausted { .. }) {
        return XmtpError::RecoveryExhausted(details(
            "recoveryExhausted",
            ErrorCategory::Stream,
            true,
            error.to_string(),
        ));
    }
    if let Some(auth) = auth_cause(&error) {
        let message = error.to_string();
        return match auth {
            AuthError::CredentialRejected { .. } => XmtpError::CredentialRejected(details(
                "credentialRejected",
                ErrorCategory::Stream,
                false,
                message,
            )),
            AuthError::Exhausted | AuthError::ExhaustedAfterAttempt => {
                XmtpError::CredentialExhausted(details(
                    "credentialExhausted",
                    ErrorCategory::Stream,
                    false,
                    message,
                ))
            }
            _ => XmtpError::unknown(error),
        };
    }
    let message = error.to_string();
    match cause {
        LocalDeliveryError::Configuration(cause) => configuration_error(cause, message),
        LocalDeliveryError::Storage(StorageError::Stream(
            StreamStorageError::AlreadyActive | StreamStorageError::NotCurrentOwner,
        )) => XmtpError::ConsumerOwned(details(
            "consumerOwned",
            ErrorCategory::Stream,
            false,
            message,
        )),
        LocalDeliveryError::Storage(StorageError::Stream(StreamStorageError::ForeignCursor)) => {
            XmtpError::ForeignCursor(details(
                "foreignCursor",
                ErrorCategory::Stream,
                false,
                message,
            ))
        }
        LocalDeliveryError::Storage(_) | LocalDeliveryError::AcknowledgementFailed => {
            XmtpError::Storage(details("storage", ErrorCategory::Storage, true, message))
        }
        _ => XmtpError::unknown(error),
    }
}

fn auth_cause(error: &(dyn Error + 'static)) -> Option<AuthError> {
    if let Some(auth) = error.downcast_ref::<AuthError>() {
        return Some(*auth);
    }
    if let Some(ApiClientError::Auth(auth)) = error.downcast_ref::<ApiClientError>() {
        return Some(*auth);
    }
    if let Some(xmtp_api::ApiError::Auth(auth)) = error.downcast_ref::<xmtp_api::ApiError>() {
        return Some(*auth);
    }
    if let Some(api) = error.downcast_ref::<ApiClientError>() {
        match api {
            ApiClientError::Other(inner) => return auth_cause(inner.as_ref()),
            ApiClientError::OtherUnretryable(inner) => return auth_cause(inner.as_ref()),
            _ => {}
        }
    }
    error.source().and_then(auth_cause)
}

pub(crate) fn configuration_error(error: &ClientError, message: String) -> XmtpError {
    match error {
        ClientError::BackendMismatch { .. } => XmtpError::BackendMismatch(details(
            "backendMismatch",
            ErrorCategory::Configuration,
            false,
            message,
        )),
        ClientError::ClientVersionTooOld { .. } => XmtpError::ClientVersionTooOld(details(
            "clientVersionTooOld",
            ErrorCategory::Configuration,
            false,
            message,
        )),
        _ => XmtpError::unknown(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[xmtp_common::test(unwrap_try = true)]
    fn close_reason_closed_and_failed() {
        use std::sync::Arc;
        use xmtp_mls::subscriptions::incoming::IncomingError;
        use xmtp_proto::api::NetworkError;

        let exhausted = delivery_error(LocalDeliveryError::NetworkRecoveryExhausted {
            attempts: 10,
            source: None,
        });
        assert!(
            matches!(exhausted, XmtpError::RecoveryExhausted(ref details)
            if details.code == "recoveryExhausted" && details.retryable)
        );
        let exhausted_with_auth = delivery_error(LocalDeliveryError::NetworkRecoveryExhausted {
            attempts: 10,
            source: Some(Arc::new(IncomingError::Transport(NetworkError::new(
                ApiClientError::Auth(AuthError::CredentialRejected { retryable: true }),
            )))),
        });
        assert!(matches!(exhausted_with_auth, XmtpError::RecoveryExhausted(ref details)
            if details.code == "recoveryExhausted" && details.retryable));
        let storage = delivery_error(LocalDeliveryError::Storage(StorageError::Stream(
            StreamStorageError::LocalReadCapacity { bytes: 2, limit: 1 },
        )));
        assert!(matches!(storage, XmtpError::Storage(ref details) if details.code == "storage"));
        let owned = delivery_error(LocalDeliveryError::Storage(StorageError::Stream(
            StreamStorageError::AlreadyActive,
        )));
        assert!(matches!(owned, XmtpError::ConsumerOwned(ref details)
            if details.code == "consumerOwned" && !details.retryable));
        let cursor = delivery_error(LocalDeliveryError::Storage(StorageError::Stream(
            StreamStorageError::ForeignCursor,
        )));
        assert!(matches!(cursor, XmtpError::ForeignCursor(ref details)
            if details.code == "foreignCursor" && !details.retryable));
        let mismatch = configuration_error(
            &ClientError::BackendMismatch {
                stored: "a".into(),
                received: "b".into(),
            },
            "mismatch".into(),
        );
        assert!(matches!(mismatch, XmtpError::BackendMismatch(ref details)
            if details.code == "backendMismatch"));
        let old = configuration_error(
            &ClientError::ClientVersionTooOld {
                client: "1".into(),
                minimum: "2".into(),
            },
            "old".into(),
        );
        assert!(matches!(old, XmtpError::ClientVersionTooOld(ref details)
            if details.code == "clientVersionTooOld"));
        for (auth, code) in [
            (
                AuthError::CredentialRejected { retryable: false },
                "credentialRejected",
            ),
            (AuthError::Exhausted, "credentialExhausted"),
        ] {
            let failure = delivery_error(LocalDeliveryError::NetworkFailure(Arc::new(
                IncomingError::Transport(NetworkError::new(ApiClientError::Auth(auth))),
            )));
            let actual = match failure {
                XmtpError::CredentialRejected(details)
                | XmtpError::CredentialExhausted(details) => details,
                other => panic!("unexpected stream failure: {other}"),
            };
            assert_eq!(actual.code, code);
            assert!(!actual.retryable);
        }
    }
}
