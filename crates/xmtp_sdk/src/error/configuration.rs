//! Public codes for server-configuration failures. They reach the error walk
//! as a `ServerConfigError` or a `BlockedConnection` from configuration,
//! preflight, refresh and stream paths, and as the matching `ClientError`
//! variants from client calls. Each maps to the same code either way.

use super::{ErrorCategory, XmtpError};

impl XmtpError {
    /// The code of a server-config root or a blocked connection at this
    /// level of the error walk.
    pub(super) fn server_config_level(error: &(dyn std::error::Error + 'static)) -> Option<Self> {
        use xmtp_mls::server_configuration::{BlockedConnection, ServerConfigError};
        if let Some(error) = error.downcast_ref::<ServerConfigError>() {
            return Some(Self::server_config_cause(error));
        }
        error
            .downcast_ref::<BlockedConnection>()
            .map(Self::blocked_cause)
    }

    pub(super) fn configuration_unavailable(
        source: &xmtp_mls::server_configuration::ConfigurationFetchError,
    ) -> Self {
        use xmtp_common::RetryableError;
        Self::ConfigurationUnavailable(Self::details(
            "ConfigurationUnavailable",
            ErrorCategory::Configuration,
            source.is_retryable(),
            source.to_string(),
        ))
    }

    pub(super) fn configuration_invalid(
        source: &xmtp_configuration::ServerConfigurationError,
    ) -> Self {
        Self::ConfigurationInvalid(Self::details(
            "ConfigurationInvalid",
            ErrorCategory::Configuration,
            false,
            source.to_string(),
        ))
    }

    pub(super) fn backend_mismatch(stored: &str, received: &str) -> Self {
        Self::BackendMismatch(Self::details(
            "BackendMismatch",
            ErrorCategory::Configuration,
            false,
            format!("database is bound to {stored}; {received} answered"),
        ))
    }

    pub(super) fn client_version_too_old(client: &str, minimum: &str) -> Self {
        Self::ClientVersionTooOld(Self::details(
            "ClientVersionTooOld",
            ErrorCategory::Configuration,
            false,
            format!("client {client} is below required version {minimum}"),
        ))
    }

    fn blocked_cause(error: &xmtp_mls::server_configuration::BlockedConnection) -> Self {
        use xmtp_mls::server_configuration::BlockedConnection;
        match error {
            BlockedConnection::BackendMismatch { stored, received } => {
                Self::backend_mismatch(stored, received)
            }
            BlockedConnection::ClientVersionTooOld { client, minimum } => {
                Self::client_version_too_old(client, minimum)
            }
        }
    }

    fn server_config_cause(error: &xmtp_mls::server_configuration::ServerConfigError) -> Self {
        use xmtp_mls::server_configuration::ServerConfigError;
        match error {
            ServerConfigError::Unavailable(source) => Self::configuration_unavailable(source),
            ServerConfigError::Invalid(source) => Self::configuration_invalid(source),
            ServerConfigError::Blocked(blocked) => Self::blocked_cause(blocked),
            ServerConfigError::Closed => Self::closed(),
        }
    }
}
