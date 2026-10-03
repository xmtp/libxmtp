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
    /// Fixed processing obligations that the operation did not complete.
    #[uniffi(default = None)]
    pub stream_failure: Option<StreamFailureDetails>,
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
    #[error("storage location unusable: {0:?}")]
    StorageLocation(ErrorDetails),
    #[error("attachment failed: {0:?}")]
    Attachment(ErrorDetails, AttachmentFailure),
    /// The identity does not belong to the inbox stored in an `Explicit`
    /// database, so the SDK opened no client.
    #[error("identity mismatch: {0:?}")]
    IdentityMismatch(ErrorDetails),
    #[error("unknown metadata field: {0:?}")]
    UnknownField(ErrorDetails),
    #[error("not a user field: {0:?}")]
    NotUserField(ErrorDetails),
    #[error("metadata field repeated: {0:?}")]
    DuplicateField(ErrorDetails),
    #[error("unsupported metadata field type: {0:?}")]
    UnsupportedType(ErrorDetails),
    #[error("metadata value type mismatch: {0:?}")]
    TypeMismatch(ErrorDetails),
    #[error("metadata field type changed: {0:?}")]
    TypeChanged(ErrorDetails),
    /// A host content codec, or its fallback or push hook, failed before the
    /// send. The SDK made no publish attempt. The host runtime returns it.
    #[error("codec encode failed: {0:?}")]
    CodecEncodeFailed(ErrorDetails),
    /// The received type has no decoder. Input error; not retryable.
    #[error("content codec not found: {0:?}")]
    CodecNotFound(ErrorDetails),
    /// Content or decompression failed. A core failure is Input; a host
    /// decoder failure is Callback. Not retryable. Receive paths keep bytes.
    #[error("content decode failed: {0:?}")]
    CodecDecodeFailed(ErrorDetails),
    /// The serialization or type is incomplete or invalid. Input error;
    /// not retryable. Receive paths keep the original bytes.
    #[error("malformed content envelope: {0:?}")]
    MalformedEnvelope(ErrorDetails),
    /// The send was published, but the SDK could not confirm its processing.
    /// Run the conversation's sync to finish it; do not send it again.
    #[error("published but unconfirmed: {0:?}")]
    PublishedButUnconfirmed(ErrorDetails),
    /// The change would make the group larger than its member limit.
    #[error("user limit exceeded: {0:?}")]
    UserLimitExceeded(ErrorDetails),
    /// A binding operation was cancelled before it finished. Partial effects and
    /// retry safety depend on the operation. The browser runtime returns it.
    #[error("cancelled: {0:?}")]
    Cancelled(ErrorDetails),
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
            stream_failure: None,
        }
    }

    pub(crate) fn invalid(message: impl Into<String>) -> Self {
        Self::InvalidInput(ErrorDetails {
            code: "InvalidInput".into(),
            category: ErrorCategory::Input,
            retryable: false,
            message: message.into(),
            stream_failure: None,
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

    pub(crate) fn codec_not_found(message: impl Into<String>) -> Self {
        Self::CodecNotFound(Self::details(
            "CodecNotFound",
            ErrorCategory::Input,
            false,
            message,
        ))
    }

    pub(crate) fn codec_decode_failed(message: impl Into<String>) -> Self {
        Self::CodecDecodeFailed(Self::details(
            "CodecDecodeFailed",
            ErrorCategory::Input,
            false,
            message,
        ))
    }

    pub(crate) fn malformed_envelope(message: impl Into<String>) -> Self {
        Self::MalformedEnvelope(Self::details(
            "MalformedEnvelope",
            ErrorCategory::Input,
            false,
            message,
        ))
    }

    pub(crate) fn content_details(self) -> ErrorDetails {
        match self {
            Self::CodecNotFound(details)
            | Self::CodecDecodeFailed(details)
            | Self::MalformedEnvelope(details) => details,
            error => Self::details(
                "CodecDecodeFailed",
                ErrorCategory::Input,
                false,
                error.to_string(),
            ),
        }
    }

    pub(crate) fn closed() -> Self {
        Self::ClientClosed(ErrorDetails {
            code: "ClientClosed".into(),
            category: ErrorCategory::Lifecycle,
            retryable: false,
            message: "client is closed".into(),
            stream_failure: None,
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
            stream_failure: None,
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
        Self::Storage(Self::details(
            "Storage",
            ErrorCategory::Storage,
            Self::storage_retryable(error),
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

    pub(crate) fn identity_mismatch() -> Self {
        Self::IdentityMismatch(Self::details(
            "IdentityMismatch",
            ErrorCategory::Identity,
            false,
            "the identity is not a member of the database's inbox",
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
            stream_failure: None,
        })
    }

    /// Map a core failure to the public code of its recovery action.
    // implements: CONF-064
    pub(crate) fn from_core<E: CoreError>(error: E) -> Self {
        Self::classify(&error)
            .unwrap_or_else(|| Self::unclassified(&error))
            .with_stream_failure(&error)
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
        if let Some(error) = error.downcast_ref::<xmtp_archive::ArchiveError>() {
            return Some(error.is_retryable());
        }
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
        if let Some(xmtp_mls::identity::IdentityError::IdentifierNotInInbox { .. }) =
            error.downcast_ref::<xmtp_mls::identity::IdentityError>()
        {
            return Some(Self::identity_mismatch());
        }
        if let Some(xmtp_mls::builder::ClientBuilderError::Attachment(attachment)) =
            error.downcast_ref::<xmtp_mls::builder::ClientBuilderError>()
        {
            // The build prepares the attachments directory. Its failure means
            // the selected storage location cannot be used.
            return Some(Self::storage_location(format!(
                "attachments directory is unusable: {}",
                xmtp_attachments::AttachmentFailureCause::as_str(attachment.cause)
            )));
        }
        if let Some(group) = error.downcast_ref::<GroupError>() {
            return match group {
                GroupError::MetadataField(field) => Some(Self::from_field(field)),
                GroupError::ComponentSource(source) => Some(Self::from_component_source(source)),
                GroupError::TooManyCharacters { .. } => {
                    Some(Self::invalid_argument(group.to_string()))
                }
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
        if let Some(location) =
            error.downcast_ref::<xmtp_mls::storage_location::StorageLocationError>()
        {
            return Some(Self::storage_location(location));
        }
        if let Some(query) = error.downcast_ref::<xmtp_db::diesel::result::Error>() {
            return Some(Self::Storage(Self::details(
                "Storage",
                ErrorCategory::Storage,
                Self::query_retryable(query),
                query.to_string(),
            )));
        }
        if let Some(platform) = error.downcast_ref::<xmtp_db::PlatformStorageError>() {
            return Some(Self::Storage(Self::details(
                "Storage",
                ErrorCategory::Storage,
                Self::platform_retryable(platform),
                platform.to_string(),
            )));
        }
        if let Some(connection) = error.downcast_ref::<xmtp_db::ConnectionError>() {
            return Some(Self::Storage(Self::details(
                "Storage",
                ErrorCategory::Storage,
                Self::connection_retryable(connection),
                connection.to_string(),
            )));
        }
        if let Some(key_store) = error.downcast_ref::<xmtp_db::sql_key_store::SqlKeyStoreError>() {
            use xmtp_db::sql_key_store::SqlKeyStoreError;
            return match key_store {
                SqlKeyStoreError::Storage(_) | SqlKeyStoreError::Connection(_) => {
                    Some(Self::Storage(Self::details(
                        "Storage",
                        ErrorCategory::Storage,
                        Self::key_store_retryable(key_store),
                        key_store.to_string(),
                    )))
                }
                _ => None,
            };
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
            xmtp_api::ApiError::InvalidRequest(_) => Self::invalid_argument(api.to_string()),
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
            stream_failure: None,
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
        Self::api_cause(&error).with_stream_failure(&error)
    }

    pub(crate) fn from_builder(error: xmtp_mls::builder::ClientBuilderError) -> Self {
        Self::from_core(error)
    }

    /// Maps a group error from a read. A database failure keeps its typed
    /// cause's retry policy, except that a missing row or a value that
    /// cannot be encoded, decoded or built into a query is not retryable. A
    /// group error without a typed source is `Unknown` and not retryable.
    /// Core's retry hint is for its own sync loop, not a promise that
    /// repeating the call is safe.
    pub(crate) fn from_group(error: xmtp_mls::groups::GroupError) -> Self {
        Self::classify(&error)
            .unwrap_or_else(|| Self::unknown(&error))
            .with_stream_failure(&error)
    }

    /// Maps a group error from a send or commit. The message or intent may
    /// already be stored, queued or published when the call fails, so
    /// repeating the call can send it or apply its delta twice. A database
    /// failure or an unknown failure is therefore not retryable here.
    /// Typed credential, configuration and notification failures keep their
    /// recovery hints. A credential recovery hint does not permit the app
    /// to repeat a send that may already be queued.
    pub(crate) fn from_group_write(error: xmtp_mls::groups::GroupError) -> Self {
        match Self::from_group(error) {
            Self::Storage(details) => Self::Storage(ErrorDetails {
                retryable: false,
                ..details
            }),
            Self::Unknown(details) => Self::Unknown(ErrorDetails {
                retryable: false,
                ..details
            }),
            other => other,
        }
    }

    /// Diesel marks every query error it does not list as retryable. A missing
    /// row, or a value that cannot be encoded, decoded or built into a query,
    /// fails the same way on every attempt, so a read does not retry it.
    fn query_fails_again(error: &xmtp_db::diesel::result::Error) -> bool {
        use xmtp_db::diesel::result::Error;
        matches!(
            error,
            Error::NotFound
                | Error::DeserializationError(_)
                | Error::SerializationError(_)
                | Error::QueryBuilderError(_)
        )
    }

    fn query_retryable(error: &xmtp_db::diesel::result::Error) -> bool {
        use xmtp_common::RetryableError;
        !Self::query_fails_again(error) && error.is_retryable()
    }

    /// Any other query error keeps the platform's own policy.
    fn platform_retryable(error: &xmtp_db::PlatformStorageError) -> bool {
        use xmtp_common::RetryableError;
        match error {
            xmtp_db::PlatformStorageError::DieselResult(query)
                if Self::query_fails_again(query) =>
            {
                false
            }
            other => other.is_retryable(),
        }
    }

    fn connection_retryable(error: &xmtp_db::ConnectionError) -> bool {
        use xmtp_common::RetryableError;
        use xmtp_db::ConnectionError;
        match error {
            ConnectionError::Database(query) => Self::query_retryable(query),
            ConnectionError::Platform(platform) => Self::platform_retryable(platform),
            other => other.is_retryable(),
        }
    }

    fn storage_retryable(error: &xmtp_db::StorageError) -> bool {
        use xmtp_common::RetryableError;
        use xmtp_db::StorageError;
        match error {
            StorageError::DieselResult(query) => Self::query_retryable(query),
            StorageError::Connection(connection) => Self::connection_retryable(connection),
            StorageError::Platform(platform) => Self::platform_retryable(platform),
            StorageError::OpenMlsStorage(key_store) => Self::key_store_retryable(key_store),
            other => other.is_retryable(),
        }
    }

    fn key_store_retryable(error: &xmtp_db::sql_key_store::SqlKeyStoreError) -> bool {
        use xmtp_common::RetryableError;
        use xmtp_db::sql_key_store::SqlKeyStoreError;
        match error {
            SqlKeyStoreError::Storage(query) => Self::query_retryable(query),
            SqlKeyStoreError::Connection(connection) => Self::connection_retryable(connection),
            other => other.is_retryable(),
        }
    }

    /// A field error keeps its kind. A denied write is the conversation's
    /// `PermissionDenied`.
    fn from_field(error: &xmtp_mls::mls_common::app_data::fields::FieldError) -> Self {
        use xmtp_mls::mls_common::app_data::fields::FieldError;
        let message = error.to_string();
        let input = |code| Self::details(code, ErrorCategory::Input, false, message.clone());
        let conversation =
            |code| Self::details(code, ErrorCategory::Conversation, false, message.clone());
        match error {
            FieldError::UnknownField(_) => Self::UnknownField(input("UnknownField")),
            FieldError::NotUserField(_) => Self::NotUserField(input("NotUserField")),
            FieldError::DuplicateField(_) => Self::DuplicateField(input("DuplicateField")),
            FieldError::TypeMismatch(_) => Self::TypeMismatch(input("TypeMismatch")),
            FieldError::UnsupportedType { .. } => {
                Self::UnsupportedType(conversation("UnsupportedType"))
            }
            FieldError::TypeChanged { .. } => Self::TypeChanged(conversation("TypeChanged")),
            FieldError::Denied(_) => Self::conversation_permission_denied(message.clone()),
            FieldError::Component(source) => Self::from_component_source(source),
        }
    }

    fn from_component_source(
        error: &xmtp_mls::mls_common::app_data::component_source::ComponentSourceError,
    ) -> Self {
        Self::Unknown(Self::details(
            "Unknown",
            ErrorCategory::Conversation,
            false,
            error.to_string(),
        ))
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
            ))
            .with_stream_failure(&source),
        }
    }
}

// Keep exported module paths stable for generated bindings.
include!("error/attachment.rs");
include!("error/stream_failure.rs");

#[cfg(test)]
mod stream_failure_tests;
#[cfg(test)]
mod tests;
