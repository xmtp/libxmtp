//! Public codes for group failures that need their own mapping: a group
//! field error keeps its kind, and a delete whose send attempt is saved is
//! retryable input.

use super::{ErrorCategory, XmtpError};
use xmtp_mls::groups::DeleteMessageError;

#[cfg_attr(feature = "pure-only", allow(dead_code))]
impl XmtpError {
    /// A field error keeps its kind. A denied write is the conversation's
    /// `PermissionDenied`.
    pub(super) fn from_field(error: &xmtp_mls::mls_common::app_data::fields::FieldError) -> Self {
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

    pub(super) fn from_component_source(
        error: &xmtp_mls::mls_common::app_data::component_source::ComponentSourceError,
    ) -> Self {
        Self::Unknown(Self::details(
            "Unknown",
            ErrorCategory::Conversation,
            false,
            error.to_string(),
        ))
    }

    /// A delete of a message whose send attempt is saved. A retry after the
    /// send's echo deletes it for everyone.
    // implements: SEND-026
    pub(super) fn from_delete(error: &DeleteMessageError) -> Option<Self> {
        matches!(error, DeleteMessageError::MessageInFlight).then(|| {
            Self::InvalidInput(Self::details(
                "MessageInFlight",
                ErrorCategory::Input,
                true,
                error.to_string(),
            ))
        })
    }
}
