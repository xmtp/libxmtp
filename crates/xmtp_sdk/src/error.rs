mod wrappers;
pub(crate) use wrappers::CoreError;
#[cfg(all(test, not(feature = "pure-only")))]
pub(crate) use wrappers::{CORE_ERROR_ROOTS, OPENED_WRAPPERS};

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
    #[error("identity not found: {0:?}")]
    IdentityNotFound(ErrorDetails),
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
    #[error("callback failed: {0:?}")]
    CallbackFailed(ErrorDetails),
    #[error("credential attempts exhausted: {0:?}")]
    CredentialExhausted(ErrorDetails),
    #[error("credential missing: {0:?}")]
    CredentialMissing(ErrorDetails),
    #[error("permission denied: {0:?}")]
    PermissionDenied(ErrorDetails),
    #[error("invalid argument: {0:?}")]
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
    #[error("invalid cursor: {0:?}")]
    InvalidCursor(ErrorDetails),
    #[error("foreign cursor: {0:?}")]
    ForeignCursor(ErrorDetails),
    /// A host content codec, or its fallback or push hook, failed before the
    /// send. The SDK made no publish attempt. The host runtime returns it.
    #[error("codec encode failed: {0:?}")]
    CodecEncodeFailed(ErrorDetails),
    /// The send was published, but the SDK could not confirm its processing.
    /// Run the conversation's sync to finish it; do not send it again.
    #[error("published but unconfirmed: {0:?}")]
    PublishedButUnconfirmed(ErrorDetails),
    /// The change would make the group larger than its member limit.
    #[error("user limit exceeded: {0:?}")]
    UserLimitExceeded(ErrorDetails),
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

    pub(crate) fn invalid_argument(message: impl Into<String>) -> Self {
        Self::InvalidArgument(Self::details(
            "InvalidArgument",
            ErrorCategory::Input,
            false,
            message,
        ))
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

    /// A storage failure with no typed retry policy, such as a file-system
    /// check. A typed `StorageError` goes through `from_core`, which keeps its
    /// retry policy.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub(crate) fn storage(error: impl std::fmt::Display) -> Self {
        Self::Storage(Self::details(
            "Storage",
            ErrorCategory::Storage,
            false,
            error.to_string(),
        ))
    }

    fn storage_cause(error: &xmtp_db::StorageError) -> Self {
        use xmtp_common::RetryableError;
        Self::Storage(Self::details(
            "Storage",
            ErrorCategory::Storage,
            error.is_retryable(),
            error.to_string(),
        ))
    }

    pub(crate) fn identity_not_found() -> Self {
        Self::IdentityNotFound(Self::details(
            "IdentityNotFound",
            ErrorCategory::Identity,
            false,
            "database has no stored identity",
        ))
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn storage_busy(message: impl Into<String>) -> Self {
        Self::StorageBusy(Self::details(
            "StorageBusy",
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

    pub(crate) fn from_group(error: xmtp_mls::groups::GroupError) -> Self {
        Self::from_core(error)
    }

    /// Map a core failure to the public code of its recovery action. The
    /// search follows the source chain, because core wraps a typed cause, such
    /// as a storage error, a configuration check, or a credential failure, in
    /// operation errors. A failure with no typed cause is `Unknown`.
    // implements: CONF-064
    pub(crate) fn from_core<E: CoreError>(error: E) -> Self {
        Self::classify(&error).unwrap_or_else(|| Self::unclassified(&error))
    }

    /// `Unknown` keeps the retry policy of a typed core error; with none, it
    /// is not retryable.
    fn unclassified(error: &(dyn std::error::Error + 'static)) -> Self {
        Self::Unknown(Self::details(
            "Unknown",
            ErrorCategory::Unknown,
            Self::typed_retryable(error).unwrap_or(false),
            error.to_string(),
        ))
    }

    fn typed_retryable(error: &(dyn std::error::Error + 'static)) -> Option<bool> {
        use xmtp_common::RetryableError;
        use xmtp_mls::{
            client::ClientError, groups::GroupError, identity::IdentityError,
            mls_store::MlsStoreError, subscriptions::catch_up::CatchUpError,
        };
        if let Some(error) = error.downcast_ref::<CatchUpError>() {
            return Some(error.is_retryable());
        }
        if let Some(error) = error.downcast_ref::<GroupError>() {
            return Some(error.is_retryable());
        }
        if let Some(error) = error.downcast_ref::<ClientError>() {
            return Some(error.is_retryable());
        }
        if let Some(error) = error.downcast_ref::<IdentityError>() {
            return Some(error.is_retryable());
        }
        if let Some(error) = error.downcast_ref::<MlsStoreError>() {
            return Some(error.is_retryable());
        }
        if let Some(error) = error.downcast_ref::<xmtp_api::ApiError>() {
            return Some(error.is_retryable());
        }
        error
            .downcast_ref::<xmtp_db::StorageError>()
            .map(RetryableError::is_retryable)
    }

    fn classify(error: &(dyn std::error::Error + 'static)) -> Option<Self> {
        // A failed check before the request wins: the request was not sent,
        // and the check's own cause names the action. The search also opens
        // the boxes that hide a source.
        if let Some(preflight) = xmtp_api::preflight::failure(error) {
            return Some(Self::preflight_cause(preflight));
        }
        let mut current = Some(error);
        while let Some(error) = current {
            if let Some(found) = Self::classify_one(error) {
                return Some(found);
            }
            current = wrappers::wrapped(error).or_else(|| error.source());
        }
        None
    }

    fn classify_one(error: &(dyn std::error::Error + 'static)) -> Option<Self> {
        use xmtp_mls::groups::GroupError;
        if let Some(group) = error.downcast_ref::<GroupError>() {
            return match group {
                GroupError::ReservedTranscriptContentType => {
                    Some(Self::InvalidInput(Self::details(
                        "ReservedTranscriptContentType",
                        ErrorCategory::Input,
                        false,
                        "reserved transcript content type",
                    )))
                }
                GroupError::UserLimitExceeded => Some(Self::UserLimitExceeded(Self::details(
                    "UserLimitExceeded",
                    ErrorCategory::Input,
                    false,
                    group.to_string(),
                ))),
                // Core may retry the confirmation, but the app must not
                // repeat the send: the conversation's sync finishes it.
                GroupError::PublishedButUnconfirmed { .. } => {
                    Some(Self::PublishedButUnconfirmed(Self::details(
                        "PublishedButUnconfirmed",
                        ErrorCategory::Conversation,
                        false,
                        group.to_string(),
                    )))
                }
                GroupError::WrappedApi(api) => Some(Self::api_cause(api)),
                GroupError::Storage(storage) => Some(Self::storage_cause(storage)),
                GroupError::Client(client) => Self::client_cause(client),
                _ => None,
            };
        }
        if let Some(client) = error.downcast_ref::<xmtp_mls::client::ClientError>() {
            return Self::client_cause(client);
        }
        if let Some(api) = error.downcast_ref::<xmtp_api::ApiError>() {
            return Some(Self::api_cause(api));
        }
        if let Some(auth) = error.downcast_ref::<xmtp_proto::api::AuthError>() {
            return Some(Self::from_auth(*auth));
        }
        if let Some(storage) = error.downcast_ref::<xmtp_db::StorageError>() {
            return Some(Self::storage_cause(storage));
        }
        // A data directory bound to another deployment has the same action
        // as a backend mismatch: select the bound deployment.
        if let Some(xmtp_mls::storage_location::StorageLocationError::DeploymentMismatch) =
            error.downcast_ref::<xmtp_mls::storage_location::StorageLocationError>()
        {
            return Some(Self::BackendMismatch(Self::details(
                "BackendMismatch",
                ErrorCategory::Configuration,
                false,
                error.to_string(),
            )));
        }
        if let Some(query) = error.downcast_ref::<xmtp_db::diesel::result::Error>() {
            use xmtp_common::RetryableError;
            return Some(Self::Storage(Self::details(
                "Storage",
                ErrorCategory::Storage,
                query.is_retryable(),
                query.to_string(),
            )));
        }
        if let Some(platform) = error.downcast_ref::<xmtp_db::PlatformStorageError>() {
            use xmtp_common::RetryableError;
            return Some(Self::Storage(Self::details(
                "Storage",
                ErrorCategory::Storage,
                platform.is_retryable(),
                platform.to_string(),
            )));
        }
        if let Some(connection) = error.downcast_ref::<xmtp_db::ConnectionError>() {
            use xmtp_common::RetryableError;
            return Some(Self::Storage(Self::details(
                "Storage",
                ErrorCategory::Storage,
                connection.is_retryable(),
                connection.to_string(),
            )));
        }
        None
    }

    /// A failed configuration check before a request keeps the
    /// code of the check's own failure.
    // implements: CONF-077
    fn preflight_cause(preflight: &xmtp_api::preflight::PreflightError) -> Self {
        use xmtp_common::RetryableError;
        std::error::Error::source(preflight)
            .and_then(Self::classify)
            .unwrap_or_else(|| {
                Self::Unknown(Self::details(
                    "Unknown",
                    ErrorCategory::Network,
                    preflight.is_retryable(),
                    preflight.to_string(),
                ))
            })
    }

    fn api_cause(api: &xmtp_api::ApiError) -> Self {
        use xmtp_common::RetryableError;
        match api {
            xmtp_api::ApiError::Auth(auth) => Self::from_auth(*auth),
            xmtp_api::ApiError::Preflight(preflight) => Self::preflight_cause(preflight),
            other => Self::classify_sources(other).unwrap_or_else(|| {
                Self::Unknown(Self::details(
                    "Unknown",
                    ErrorCategory::Network,
                    other.is_retryable(),
                    other.to_string(),
                ))
            }),
        }
    }

    /// The configuration or lifecycle code of a typed cause in `error`'s
    /// chain, if it has one.
    #[cfg(not(target_arch = "wasm32"))]
    fn configuration_cause(error: &(dyn std::error::Error + 'static)) -> Option<Self> {
        Self::classify(error).filter(|found| {
            matches!(
                found,
                Self::BackendMismatch(_)
                    | Self::ClientVersionTooOld(_)
                    | Self::ConfigurationUnavailable(_)
                    | Self::ConfigurationInvalid(_)
                    | Self::AuthRequired(_)
                    | Self::ChainNotAccepted(_)
                    | Self::ClientClosed(_)
            )
        })
    }

    /// Classify only the causes below `error`, not `error` itself.
    fn classify_sources(error: &(dyn std::error::Error + 'static)) -> Option<Self> {
        error.source().and_then(Self::classify)
    }

    pub(crate) fn client_cause(error: &xmtp_mls::client::ClientError) -> Option<Self> {
        use xmtp_common::RetryableError;
        use xmtp_mls::client::ClientError;
        Some(match error {
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
            ClientError::Api(api) => Self::api_cause(api),
            ClientError::Storage(storage) => Self::storage_cause(storage),
            _ => return None,
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

    pub(crate) fn callback_failed() -> Self {
        Self::CallbackFailed(Self::details(
            "CallbackFailed",
            ErrorCategory::Callback,
            false,
            "pre-authenticate callback failed",
        ))
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
        Self::from_core(error)
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
        Self::api_cause(&error)
    }

    pub(crate) fn from_builder(error: xmtp_mls::builder::ClientBuilderError) -> Self {
        Self::from_core(error)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn from_notification(
        error: xmtp_mls::client::notifications::NotificationError,
    ) -> Self {
        use xmtp_common::RetryableError;
        use xmtp_mls::client::notifications::NotificationError;
        // A failed configuration check or a blocked connection keeps its
        // configuration code, as on every other path.
        let configuration = match &error {
            NotificationError::Api(source) => Self::configuration_cause(source),
            NotificationError::Group(source) => Self::configuration_cause(source),
            _ => None,
        };
        if let Some(found) = configuration {
            return found;
        }
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

#[cfg(test)]
mod tests {
    use super::{ErrorCategory, XmtpError};

    #[xmtp_common::test(unwrap_try = true)]
    fn storage_busy_matches_browser_bridge_fields() {
        let XmtpError::StorageBusy(details) = XmtpError::storage_busy("busy") else {
            panic!("expected StorageBusy");
        };
        assert_eq!(details.code, "StorageBusy");
        assert!(matches!(details.category, ErrorCategory::Storage));
        assert!(details.retryable);
    }

    // verifies: GMOD-035
    #[xmtp_common::test(unwrap_try = true)]
    fn reserved_transcript_type_is_a_stable_input_error() {
        use xmtp_mls::groups::GroupError;

        let XmtpError::InvalidInput(refused) =
            XmtpError::from_group(GroupError::ReservedTranscriptContentType)
        else {
            panic!("expected InvalidInput");
        };
        assert_eq!(refused.code, "ReservedTranscriptContentType");
        assert!(matches!(refused.category, ErrorCategory::Input));
        assert!(!refused.retryable);
    }
}
