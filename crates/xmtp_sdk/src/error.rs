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
    Configuration,
    Notification,
    Stream,
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
    #[error("storage location required: {0:?}")]
    StorageLocationRequired(ErrorDetails),
    #[error("storage pool busy: {0:?}")]
    StorageBusy(ErrorDetails),
    #[error("signer failed: {0:?}")]
    Signer(ErrorDetails),
    #[error("credential failed: {0:?}")]
    Credential(ErrorDetails),
    #[error("configuration unavailable: {0:?}")]
    ConfigurationUnavailable(ErrorDetails),
    #[error("configuration invalid: {0:?}")]
    ConfigurationInvalid(ErrorDetails),
    #[error("backend mismatch: {0:?}")]
    BackendMismatch(ErrorDetails),
    #[error("client version too old: {0:?}")]
    ClientVersionTooOld(ErrorDetails),
    #[error("authentication required: {0:?}")]
    AuthRequired(ErrorDetails),
    #[error("chain not accepted: {0:?}")]
    ChainNotAccepted(ErrorDetails),
    #[error("credential rejected: {0:?}")]
    CredentialRejected(ErrorDetails),
    #[error("credential callback failed: {0:?}")]
    CredentialCallbackFailed(ErrorDetails),
    #[error("credential attempts exhausted: {0:?}")]
    CredentialExhausted(ErrorDetails),
    #[error("credential missing: {0:?}")]
    CredentialMissing(ErrorDetails),
    #[error("permission denied: {0:?}")]
    PermissionDenied(ErrorDetails),
    #[error("notification argument invalid: {0:?}")]
    InvalidArgument(ErrorDetails),
    #[error("notification value out of range: {0:?}")]
    OutOfRange(ErrorDetails),
    #[error("notification service unavailable: {0:?}")]
    Unimplemented(ErrorDetails),
    #[error("notification channel not configured: {0:?}")]
    ChannelNotConfigured(ErrorDetails),
    #[error("notification task runner disabled: {0:?}")]
    TaskRunnerDisabled(ErrorDetails),
    #[error("notification topic limit reached: {0:?}")]
    ResourceExhausted(ErrorDetails),
    #[error("notification request timed out: {0:?}")]
    RequestTimeout(ErrorDetails),
    #[error("notification recipient missing: {0:?}")]
    NotificationNotFound(ErrorDetails),
    #[error("notification request failed: {0:?}")]
    NotificationApi(ErrorDetails),
    #[error("notification storage failed: {0:?}")]
    NotificationStorage(ErrorDetails),
    #[error("notification group failed: {0:?}")]
    NotificationGroup(ErrorDetails),
    #[error("stream recovery exhausted: {0:?}")]
    RecoveryExhausted(ErrorDetails),
    #[error("stream storage failure: {0:?}")]
    Storage(ErrorDetails),
    #[error("stream lagged: {0:?}")]
    Lagged(ErrorDetails),
    #[error("stream consumer owned: {0:?}")]
    ConsumerOwned(ErrorDetails),
    #[error("foreign cursor: {0:?}")]
    ForeignCursor(ErrorDetails),
    #[error("unknown failure: {0:?}")]
    Unknown(ErrorDetails),
}

#[cfg_attr(feature = "pure-only", allow(dead_code))]
impl XmtpError {
    fn details(
        code: &str,
        category: ErrorCategory,
        retryable: bool,
        message: impl Into<String>,
    ) -> ErrorDetails {
        ErrorDetails {
            code: code.into(),
            category,
            retryable,
            message: message.into(),
        }
    }

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

    pub(crate) fn conversation_permission_denied(message: impl Into<String>) -> Self {
        Self::PermissionDenied(Self::details(
            "PermissionDenied",
            ErrorCategory::Conversation,
            false,
            message,
        ))
    }

    pub(crate) fn storage_location_required() -> Self {
        Self::StorageLocationRequired(ErrorDetails {
            code: "StorageLocationRequired".into(),
            category: ErrorCategory::Storage,
            retryable: false,
            message: "the host must resolve the default storage location".into(),
        })
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn storage_busy(message: impl Into<String>) -> Self {
        Self::StorageBusy(Self::details(
            "storageBusy",
            ErrorCategory::Storage,
            true,
            message,
        ))
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

    pub(crate) fn from_signature_request(
        error: xmtp_id::associations::builder::SignatureRequestError,
    ) -> Self {
        match &error {
            xmtp_id::associations::builder::SignatureRequestError::ChainNotAccepted { .. } => {
                Self::ChainNotAccepted(Self::details(
                    "ChainNotAccepted",
                    ErrorCategory::Configuration,
                    false,
                    error.to_string(),
                ))
            }
            _ => Self::invalid(error.to_string()),
        }
    }

    pub(crate) fn from_client(error: xmtp_mls::client::ClientError) -> Self {
        use xmtp_common::RetryableError;
        use xmtp_mls::client::ClientError;
        match error {
            ClientError::AlreadyClosed => Self::closed(),
            ClientError::ConfigurationUnavailable(source) => {
                Self::ConfigurationUnavailable(Self::details(
                    "ConfigurationUnavailable",
                    ErrorCategory::Configuration,
                    source.is_retryable(),
                    source.to_string(),
                ))
            }
            ClientError::ConfigurationInvalid(source) => Self::ConfigurationInvalid(Self::details(
                "ConfigurationInvalid",
                ErrorCategory::Configuration,
                false,
                source.to_string(),
            )),
            ClientError::BackendMismatch { stored, received } => {
                Self::BackendMismatch(Self::details(
                    "BackendMismatch",
                    ErrorCategory::Configuration,
                    false,
                    format!("database is bound to {stored}; {received} answered"),
                ))
            }
            ClientError::ClientVersionTooOld { client, minimum } => {
                Self::ClientVersionTooOld(Self::details(
                    "ClientVersionTooOld",
                    ErrorCategory::Configuration,
                    false,
                    format!("client {client} is below required version {minimum}"),
                ))
            }
            ClientError::AuthRequired { required_scopes } => Self::AuthRequired(Self::details(
                "AuthRequired",
                ErrorCategory::Configuration,
                false,
                format!("required scopes: {}", required_scopes.join(", ")),
            )),
            ClientError::ChainNotAccepted { chain, accepted } => {
                Self::ChainNotAccepted(Self::details(
                    "ChainNotAccepted",
                    ErrorCategory::Configuration,
                    false,
                    format!("chain {chain} is not in {}", accepted.join(", ")),
                ))
            }
            ClientError::Api(api) => Self::from_api(api),
            ClientError::Identity(identity) => Self::from_identity(identity),
            other => {
                let retryable = other.is_retryable();
                Self::Unknown(Self::details(
                    "Unknown",
                    ErrorCategory::Unknown,
                    retryable,
                    other.to_string(),
                ))
            }
        }
    }

    pub(crate) fn from_auth(error: xmtp_proto::api::AuthError) -> Self {
        use xmtp_proto::api::AuthError;
        match error {
            AuthError::CredentialRejected { retryable } => Self::CredentialRejected(Self::details(
                "CredentialRejected",
                ErrorCategory::Callback,
                retryable,
                "credential rejected",
            )),
            AuthError::CallbackFailed { retryable } => {
                Self::CredentialCallbackFailed(Self::details(
                    "CredentialCallbackFailed",
                    ErrorCategory::Callback,
                    retryable,
                    "credential callback failed",
                ))
            }
            AuthError::Exhausted | AuthError::ExhaustedAfterAttempt => {
                Self::CredentialExhausted(Self::details(
                    "CredentialExhausted",
                    ErrorCategory::Callback,
                    false,
                    "credential attempts exhausted",
                ))
            }
            AuthError::MissingCredential => Self::CredentialMissing(Self::details(
                "CredentialMissing",
                ErrorCategory::Callback,
                false,
                "credential missing",
            )),
        }
    }

    pub(crate) fn from_api(error: xmtp_api::ApiError) -> Self {
        use xmtp_common::RetryableError;
        match error {
            xmtp_api::ApiError::Auth(auth) => Self::from_auth(auth),
            other => Self::Unknown(Self::details(
                "Unknown",
                ErrorCategory::Network,
                other.is_retryable(),
                other.to_string(),
            )),
        }
    }

    fn from_identity(error: xmtp_mls::identity::IdentityError) -> Self {
        match error {
            xmtp_mls::identity::IdentityError::ApiClient(api) => Self::from_api(api),
            other => Self::unknown(other),
        }
    }

    pub(crate) fn from_builder(error: xmtp_mls::builder::ClientBuilderError) -> Self {
        use xmtp_mls::builder::ClientBuilderError;
        match error {
            ClientBuilderError::WrappedApiError(api) => Self::from_api(api),
            ClientBuilderError::ClientError(client) => Self::from_client(client),
            ClientBuilderError::Identity(identity) => Self::from_identity(identity),
            other => Self::unknown(other),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn from_notification(
        error: xmtp_mls::client::notifications::NotificationError,
    ) -> Self {
        use xmtp_common::RetryableError;
        use xmtp_mls::client::notifications::NotificationError;
        match error {
            NotificationError::PermissionDenied => Self::PermissionDenied(Self::details(
                "PermissionDenied",
                ErrorCategory::Notification,
                false,
                "notification permission denied",
            )),
            NotificationError::InvalidArgument => Self::InvalidArgument(Self::details(
                "InvalidArgument",
                ErrorCategory::Notification,
                false,
                "invalid notification argument",
            )),
            NotificationError::OutOfRange => Self::OutOfRange(Self::details(
                "OutOfRange",
                ErrorCategory::Notification,
                false,
                "notification value out of range",
            )),
            NotificationError::Unimplemented => Self::Unimplemented(Self::details(
                "Unimplemented",
                ErrorCategory::Notification,
                false,
                "notifications are not implemented",
            )),
            NotificationError::ChannelNotConfigured => Self::ChannelNotConfigured(Self::details(
                "ChannelNotConfigured",
                ErrorCategory::Notification,
                false,
                "notification channel not configured",
            )),
            NotificationError::TaskRunnerDisabled => Self::TaskRunnerDisabled(Self::details(
                "TaskRunnerDisabled",
                ErrorCategory::Notification,
                false,
                "notification task runner is disabled",
            )),
            NotificationError::ResourceExhausted => Self::ResourceExhausted(Self::details(
                "ResourceExhausted",
                ErrorCategory::Notification,
                false,
                "notification topic limit reached",
            )),
            NotificationError::RequestTimeout => Self::RequestTimeout(Self::details(
                "RequestTimeout",
                ErrorCategory::Notification,
                true,
                "notification request timed out",
            )),
            NotificationError::NotFound => Self::NotificationNotFound(Self::details(
                "NotificationNotFound",
                ErrorCategory::Notification,
                true,
                "notification recipient is not registered",
            )),
            NotificationError::Api(xmtp_api::ApiError::Auth(auth)) => Self::from_auth(auth),
            NotificationError::Api(source) => Self::NotificationApi(Self::details(
                "NotificationApi",
                ErrorCategory::Notification,
                source.is_retryable(),
                source.to_string(),
            )),
            NotificationError::Storage(source) => Self::NotificationStorage(Self::details(
                "NotificationStorage",
                ErrorCategory::Storage,
                source.is_retryable(),
                source.to_string(),
            )),
            NotificationError::Group(source) => Self::NotificationGroup(Self::details(
                "NotificationGroup",
                ErrorCategory::Conversation,
                source.is_retryable(),
                source.to_string(),
            )),
        }
    }
}
