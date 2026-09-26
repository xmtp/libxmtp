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
            "RecoveryExhausted",
            ErrorCategory::Stream,
            true,
            error.to_string(),
        ));
    }
    if let Some(auth) = auth_cause(&error) {
        return XmtpError::from_auth(auth);
    }
    let message = error.to_string();
    match cause {
        LocalDeliveryError::Configuration(cause) => configuration_error(cause, message),
        LocalDeliveryError::Storage(StorageError::Stream(
            StreamStorageError::AlreadyActive | StreamStorageError::NotCurrentOwner,
        )) => XmtpError::ConsumerOwned(details(
            "ConsumerOwned",
            ErrorCategory::Stream,
            false,
            message,
        )),
        LocalDeliveryError::Storage(StorageError::Stream(StreamStorageError::ForeignCursor)) => {
            XmtpError::ForeignCursor(details(
                "ForeignCursor",
                ErrorCategory::Stream,
                false,
                message,
            ))
        }
        LocalDeliveryError::Storage(_) | LocalDeliveryError::AcknowledgementFailed => {
            XmtpError::Storage(details("Storage", ErrorCategory::Storage, true, message))
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
            "BackendMismatch",
            ErrorCategory::Configuration,
            false,
            message,
        )),
        ClientError::ClientVersionTooOld { .. } => XmtpError::ClientVersionTooOld(details(
            "ClientVersionTooOld",
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
    fn stream_close_codes_match_variants() {
        let cases = [
            (
                delivery_error(LocalDeliveryError::NetworkRecoveryExhausted {
                    attempts: 1,
                    source: None,
                }),
                "RecoveryExhausted",
            ),
            (
                delivery_error(LocalDeliveryError::Storage(StorageError::Stream(
                    StreamStorageError::LocalReadCapacity { bytes: 2, limit: 1 },
                ))),
                "Storage",
            ),
            (
                delivery_error(LocalDeliveryError::Storage(StorageError::Stream(
                    StreamStorageError::AlreadyActive,
                ))),
                "ConsumerOwned",
            ),
            (
                delivery_error(LocalDeliveryError::Storage(StorageError::Stream(
                    StreamStorageError::ForeignCursor,
                ))),
                "ForeignCursor",
            ),
            (
                configuration_error(
                    &ClientError::BackendMismatch {
                        stored: "a".into(),
                        received: "b".into(),
                    },
                    "mismatch".into(),
                ),
                "BackendMismatch",
            ),
            (
                configuration_error(
                    &ClientError::ClientVersionTooOld {
                        client: "1".into(),
                        minimum: "2".into(),
                    },
                    "old".into(),
                ),
                "ClientVersionTooOld",
            ),
        ];
        for (error, expected) in cases {
            let actual = match error {
                XmtpError::RecoveryExhausted(details)
                | XmtpError::Storage(details)
                | XmtpError::ConsumerOwned(details)
                | XmtpError::ForeignCursor(details)
                | XmtpError::BackendMismatch(details)
                | XmtpError::ClientVersionTooOld(details) => details.code,
                other => panic!("unexpected close error: {other}"),
            };
            assert_eq!(actual, expected);
        }
    }

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
            if details.code == "RecoveryExhausted" && details.retryable)
        );
        let exhausted_with_auth = delivery_error(LocalDeliveryError::NetworkRecoveryExhausted {
            attempts: 10,
            source: Some(Arc::new(IncomingError::Transport(NetworkError::new(
                ApiClientError::Auth(AuthError::CredentialRejected { retryable: true }),
            )))),
        });
        assert!(
            matches!(exhausted_with_auth, XmtpError::RecoveryExhausted(ref details)
            if details.code == "RecoveryExhausted" && details.retryable)
        );
        let storage = delivery_error(LocalDeliveryError::Storage(StorageError::Stream(
            StreamStorageError::LocalReadCapacity { bytes: 2, limit: 1 },
        )));
        assert!(matches!(storage, XmtpError::Storage(ref details) if details.code == "Storage"));
        let owned = delivery_error(LocalDeliveryError::Storage(StorageError::Stream(
            StreamStorageError::AlreadyActive,
        )));
        assert!(matches!(owned, XmtpError::ConsumerOwned(ref details)
            if details.code == "ConsumerOwned" && !details.retryable));
        let cursor = delivery_error(LocalDeliveryError::Storage(StorageError::Stream(
            StreamStorageError::ForeignCursor,
        )));
        assert!(matches!(cursor, XmtpError::ForeignCursor(ref details)
            if details.code == "ForeignCursor" && !details.retryable));
        let mismatch = configuration_error(
            &ClientError::BackendMismatch {
                stored: "a".into(),
                received: "b".into(),
            },
            "mismatch".into(),
        );
        assert!(matches!(mismatch, XmtpError::BackendMismatch(ref details)
            if details.code == "BackendMismatch"));
        let old = configuration_error(
            &ClientError::ClientVersionTooOld {
                client: "1".into(),
                minimum: "2".into(),
            },
            "old".into(),
        );
        assert!(matches!(old, XmtpError::ClientVersionTooOld(ref details)
            if details.code == "ClientVersionTooOld"));
        for (auth, code, retryable) in [
            (
                AuthError::CredentialRejected { retryable: true },
                "CredentialRejected",
                true,
            ),
            (
                AuthError::CredentialRejected { retryable: false },
                "CredentialRejected",
                false,
            ),
            (
                AuthError::CallbackFailed { retryable: true },
                "CredentialCallbackFailed",
                true,
            ),
            (
                AuthError::CallbackFailed { retryable: false },
                "CredentialCallbackFailed",
                false,
            ),
            (AuthError::Exhausted, "CredentialExhausted", false),
            (
                AuthError::ExhaustedAfterAttempt,
                "CredentialExhausted",
                false,
            ),
            (AuthError::MissingCredential, "CredentialMissing", false),
        ] {
            let failure = delivery_error(LocalDeliveryError::NetworkFailure(Arc::new(
                IncomingError::Transport(NetworkError::new(ApiClientError::Auth(auth))),
            )));
            let actual = match failure {
                XmtpError::CredentialRejected(details)
                | XmtpError::CredentialCallbackFailed(details)
                | XmtpError::CredentialExhausted(details)
                | XmtpError::CredentialMissing(details) => details,
                other => panic!("unexpected stream failure: {other}"),
            };
            assert_eq!(actual.code, code);
            assert_eq!(actual.retryable, retryable, "{code} retryability");
        }
    }
}
