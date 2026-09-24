/// The kind of a façade failure.
#[derive(Clone, Debug, uniffi::Enum)]
pub enum ErrorCategory {
    Input,
    Network,
    Storage,
    Identity,
    Conversation,
    Callback,
    Lifecycle,
    Unknown,
}

/// Stable information carried by each error variant.
#[derive(Clone, Debug, uniffi::Record)]
pub struct ErrorDetails {
    pub code: String,
    pub category: ErrorCategory,
    pub retryable: bool,
    pub message: String,
}

/// Errors returned by the SDK façade.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum XmtpError {
    #[error("client closed: {0:?}")]
    ClientClosed(ErrorDetails),
    #[error("invalid input: {0:?}")]
    InvalidInput(ErrorDetails),
    #[error("signer failed: {0:?}")]
    Signer(ErrorDetails),
    #[error("credential failed: {0:?}")]
    Credential(ErrorDetails),
    #[error("unknown failure: {0:?}")]
    Unknown(ErrorDetails),
}

impl XmtpError {
    pub(crate) fn invalid(message: impl Into<String>) -> Self {
        Self::InvalidInput(ErrorDetails {
            code: "InvalidInput".into(),
            category: ErrorCategory::Input,
            retryable: false,
            message: message.into(),
        })
    }

    pub(crate) fn closed() -> Self {
        Self::ClientClosed(ErrorDetails {
            code: "ClientClosed".into(),
            category: ErrorCategory::Lifecycle,
            retryable: false,
            message: "client is closed".into(),
        })
    }

    pub(crate) fn unknown(error: impl std::fmt::Display) -> Self {
        Self::Unknown(ErrorDetails {
            code: "Unknown".into(),
            category: ErrorCategory::Unknown,
            retryable: false,
            message: error.to_string(),
        })
    }

    pub(crate) fn signer() -> Self {
        Self::Signer(ErrorDetails {
            code: "SignerFailed".into(),
            category: ErrorCategory::Callback,
            retryable: false,
            message: "signer callback failed".into(),
        })
    }
}
