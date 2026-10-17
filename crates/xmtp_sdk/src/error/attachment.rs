/// Why an attachment operation failed. Each value is one ATCH failure cause.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum AttachmentFailureCause {
    NotOffered,
    TooLarge,
    SourceUnreadable,
    LocalStorage,
    StagedUnusable,
    ConnectionBlocked,
    Credential,
    BackendRejected,
    BackendUnavailable,
    TargetRejected,
    Network,
    InsecureUrl,
    BlockedAddress,
    TooManyRedirects,
    NotFound,
    HttpStatus,
    Malformed,
    DigestMismatch,
    DecryptionFailed,
    NotAnAttachment,
    Deleted,
}

/// Which credential step failed, for the `Credential` cause.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum CredentialFailureKind {
    CredentialRejected,
    CallbackFailed,
    Exhausted,
    MissingCredential,
}

/// The failure of an attachment operation. A thrown `XmtpError.Attachment`
/// and a `Failed` pending status carry the same record.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct AttachmentFailure {
    pub cause: AttachmentFailureCause,
    pub credential_kind: Option<CredentialFailureKind>,
    /// For the `Credential` cause, whether the credential source can succeed
    /// on a later attempt, which also decides whether the operation can. For
    /// another cause the thrown error's `retryable` detail says whether the
    /// operation can.
    pub retryable: bool,
    /// The backend rejected the credential's scope.
    pub missing_scope: bool,
    /// The final status from the storage target or the download host.
    pub http_status: Option<u16>,
}

macro_rules! mapped_causes {
    ($($name:ident),+ $(,)?) => {
        impl From<xmtp_attachments::AttachmentFailureCause> for AttachmentFailureCause {
            fn from(value: xmtp_attachments::AttachmentFailureCause) -> Self {
                match value {
                    $(xmtp_attachments::AttachmentFailureCause::$name => Self::$name,)+
                }
            }
        }

        impl From<AttachmentFailureCause> for xmtp_attachments::AttachmentFailureCause {
            fn from(value: AttachmentFailureCause) -> Self {
                match value {
                    $(AttachmentFailureCause::$name => Self::$name,)+
                }
            }
        }
    };
}

mapped_causes!(
    NotOffered,
    TooLarge,
    SourceUnreadable,
    LocalStorage,
    StagedUnusable,
    ConnectionBlocked,
    Credential,
    BackendRejected,
    BackendUnavailable,
    TargetRejected,
    Network,
    InsecureUrl,
    BlockedAddress,
    TooManyRedirects,
    NotFound,
    HttpStatus,
    Malformed,
    DigestMismatch,
    DecryptionFailed,
    NotAnAttachment,
    Deleted,
);

impl From<xmtp_mls::attachments::CredentialFailureKind> for CredentialFailureKind {
    fn from(value: xmtp_mls::attachments::CredentialFailureKind) -> Self {
        use xmtp_mls::attachments::CredentialFailureKind as Core;
        match value {
            Core::CredentialRejected => Self::CredentialRejected,
            Core::CallbackFailed => Self::CallbackFailed,
            Core::Exhausted => Self::Exhausted,
            Core::MissingCredential => Self::MissingCredential,
        }
    }
}

impl From<CredentialFailureKind> for xmtp_mls::attachments::CredentialFailureKind {
    fn from(value: CredentialFailureKind) -> Self {
        match value {
            CredentialFailureKind::CredentialRejected => Self::CredentialRejected,
            CredentialFailureKind::CallbackFailed => Self::CallbackFailed,
            CredentialFailureKind::Exhausted => Self::Exhausted,
            CredentialFailureKind::MissingCredential => Self::MissingCredential,
        }
    }
}

impl From<xmtp_mls::attachments::AttachmentClientError> for AttachmentFailure {
    fn from(value: xmtp_mls::attachments::AttachmentClientError) -> Self {
        Self {
            cause: value.cause.into(),
            credential_kind: value.credential_kind.map(Into::into),
            retryable: value.retryable,
            missing_scope: value.missing_scope,
            http_status: value.http_status,
        }
    }
}

impl From<AttachmentFailure> for xmtp_mls::attachments::AttachmentClientError {
    fn from(value: AttachmentFailure) -> Self {
        Self {
            cause: value.cause.into(),
            credential_kind: value.credential_kind.map(Into::into),
            retryable: value.retryable,
            missing_scope: value.missing_scope,
            http_status: value.http_status,
        }
    }
}

impl AttachmentFailure {
    #[cfg_attr(feature = "pure-only", allow(dead_code))]
    pub(crate) fn with_cause(cause: AttachmentFailureCause) -> Self {
        Self {
            cause,
            credential_kind: None,
            retryable: false,
            missing_scope: false,
            http_status: None,
        }
    }

    #[cfg_attr(feature = "pure-only", allow(dead_code))]
    fn category(&self) -> ErrorCategory {
        use AttachmentFailureCause::*;
        match self.cause {
            NotOffered | ConnectionBlocked => ErrorCategory::Configuration,
            TooLarge | SourceUnreadable | InsecureUrl | Malformed | DigestMismatch
            | DecryptionFailed | NotAnAttachment => ErrorCategory::Input,
            LocalStorage | StagedUnusable | Deleted => ErrorCategory::Storage,
            Credential => ErrorCategory::Callback,
            BackendRejected | BackendUnavailable | TargetRejected | Network | BlockedAddress
            | TooManyRedirects | NotFound | HttpStatus => ErrorCategory::Network,
        }
    }

    /// Whether the same call can succeed later with no change by the app,
    /// by the ATCH cause table.
    #[cfg_attr(feature = "pure-only", allow(dead_code))]
    fn later_attempt_can_succeed(&self) -> bool {
        use AttachmentFailureCause::*;
        match self.cause {
            LocalStorage | BackendUnavailable | Network | TargetRejected | NotFound | Deleted => {
                true
            }
            Credential => self.retryable,
            HttpStatus => self
                .http_status
                .is_some_and(|status| matches!(status, 408 | 429 | 500..=599)),
            NotOffered | TooLarge | SourceUnreadable | StagedUnusable | ConnectionBlocked
            | BackendRejected | InsecureUrl | BlockedAddress | TooManyRedirects | Malformed
            | DigestMismatch | DecryptionFailed | NotAnAttachment => false,
        }
    }
}

impl XmtpError {
    #[cfg_attr(feature = "pure-only", allow(dead_code))]
    pub(crate) fn attachment(failure: AttachmentFailure) -> Self {
        let cause = xmtp_attachments::AttachmentFailureCause::from(failure.cause).as_str();
        let message = match failure.http_status {
            Some(status) => format!("attachment {cause} (HTTP {status})"),
            None => format!("attachment {cause}"),
        };
        Self::Attachment(
            Self::details(
                "Attachment",
                failure.category(),
                failure.later_attempt_can_succeed(),
                message,
            ),
            failure,
        )
    }

    #[cfg_attr(feature = "pure-only", allow(dead_code))]
    pub(crate) fn from_attachment(error: xmtp_mls::attachments::AttachmentClientError) -> Self {
        Self::attachment(error.into())
    }

    pub(crate) fn storage_location(message: impl std::fmt::Display) -> Self {
        Self::StorageLocation(Self::details(
            "StorageLocation",
            ErrorCategory::Storage,
            false,
            message.to_string(),
        ))
    }
}
