//! Public codes for handle failures. The backend's refusals arrive as an
//! `ApiError::HandleRejected` inside an identity update error. The client's
//! own refusals arrive as a `ClientError::Handle`. Both reach the error walk
//! through every wrapper that core puts around them, including `GroupError`,
//! because the walk classifies `ApiError` and `ClientError` wherever it finds
//! them. None of these codes is retryable: the same claim fails the same way.

use super::{ErrorCategory, XmtpError};
use xmtp_api::HandleRejection;
use xmtp_mls::inbox_handles::HandleError;

#[cfg_attr(feature = "pure-only", allow(dead_code))]
impl XmtpError {
    /// The code of a claim that the backend refused.
    pub(super) fn from_handle_rejection(rejection: HandleRejection) -> Self {
        let message = rejection.to_string();
        match rejection {
            HandleRejection::Taken => Self::HandleTaken(Self::details(
                "HandleTaken",
                ErrorCategory::Identity,
                false,
                message,
            )),
            HandleRejection::NotAllowed => Self::HandleNotAllowed(Self::details(
                "HandleNotAllowed",
                ErrorCategory::Identity,
                false,
                message,
            )),
            HandleRejection::TooSoon => Self::HandleTooSoon(Self::details(
                "HandleTooSoon",
                ErrorCategory::Identity,
                false,
                message,
            )),
        }
    }

    /// The code of a handle operation that the client refused. Text outside
    /// the handle grammar is `InvalidArgument`. A handle that no inbox holds
    /// is `IdentityNotFound`, as the static inbox lookup reports it.
    pub(super) fn from_handle(error: &HandleError) -> Self {
        let message = error.to_string();
        match error {
            HandleError::Invalid(_) => Self::invalid_argument(message),
            HandleError::NotFound => Self::IdentityNotFound(Self::details(
                "IdentityNotFound",
                ErrorCategory::Identity,
                false,
                message,
            )),
            HandleError::LimitReached => Self::HandleLimitReached(Self::details(
                "HandleLimitReached",
                ErrorCategory::Identity,
                false,
                message,
            )),
        }
    }

    /// No inbox holds the handle that a lookup named.
    pub(crate) fn handle_not_found() -> Self {
        Self::from_handle(&HandleError::NotFound)
    }
}
