//! Attachment errors: the stable cause, the credential detail and the API error mapping.

use super::*;

impl xmtp_mls_core::worker::NeedsDbReconnect for AttachmentClientError {
    fn needs_db_reconnect(&self) -> bool {
        false
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CredentialFailureKind {
    CredentialRejected,
    CallbackFailed,
    Exhausted,
    MissingCredential,
}
impl CredentialFailureKind {
    pub const ALL: [Self; 4] = [
        Self::CredentialRejected,
        Self::CallbackFailed,
        Self::Exhausted,
        Self::MissingCredential,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CredentialRejected => "credential_rejected",
            Self::CallbackFailed => "callback_failed",
            Self::Exhausted => "exhausted",
            Self::MissingCredential => "missing_credential",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "credential_rejected" => Some(Self::CredentialRejected),
            "callback_failed" => Some(Self::CallbackFailed),
            "exhausted" => Some(Self::Exhausted),
            "missing_credential" => Some(Self::MissingCredential),
            _ => None,
        }
    }
}
/// A stable attachment cause with the credential detail, when applicable.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("attachment failure: {}", cause.as_str())]
pub struct AttachmentClientError {
    pub cause: Cause,
    pub credential_kind: Option<CredentialFailureKind>,
    pub retryable: bool,
    /// True when the backend rejected the credential's scope.
    pub missing_scope: bool,
    /// Final status from the storage target or download host.
    pub http_status: Option<u16>,
}
impl AttachmentClientError {
    pub(super) fn new(cause: Cause) -> Self {
        Self {
            cause,
            credential_kind: None,
            retryable: false,
            missing_scope: false,
            http_status: None,
        }
    }
}
impl From<AttachmentError> for AttachmentClientError {
    fn from(error: AttachmentError) -> Self {
        Self {
            http_status: error.http_status,
            ..Self::new(error.cause)
        }
    }
}
impl From<StoreMoveError> for AttachmentClientError {
    fn from(error: StoreMoveError) -> Self {
        match error {
            StoreMoveError::DestinationExists => Self::new(Cause::LocalStorage),
            StoreMoveError::Other(error) => error.into(),
        }
    }
}
pub(super) fn api_error(error: xmtp_api::ApiError) -> AttachmentClientError {
    if let xmtp_api::ApiError::Auth(auth) = &error {
        use xmtp_proto::api::AuthError;
        let credential_kind = match auth {
            AuthError::CredentialRejected { .. } => CredentialFailureKind::CredentialRejected,
            AuthError::CallbackFailed { .. } => CredentialFailureKind::CallbackFailed,
            AuthError::Exhausted | AuthError::ExhaustedAfterAttempt => {
                CredentialFailureKind::Exhausted
            }
            AuthError::MissingCredential => CredentialFailureKind::MissingCredential,
        };
        return AttachmentClientError {
            cause: Cause::Credential,
            credential_kind: Some(credential_kind),
            retryable: auth.is_retryable(),
            missing_scope: false,
            http_status: None,
        };
    }
    if grpc_status(&error).is_some_and(|status| status.code() == tonic::Code::PermissionDenied) {
        return AttachmentClientError {
            missing_scope: true,
            ..AttachmentClientError::new(Cause::Credential)
        };
    }
    let rejected = grpc_status(&error).is_some_and(|status| {
        matches!(
            status.code(),
            tonic::Code::InvalidArgument | tonic::Code::OutOfRange | tonic::Code::Unimplemented
        )
    });
    AttachmentClientError::new(if rejected {
        Cause::BackendRejected
    } else {
        Cause::BackendUnavailable
    })
}
