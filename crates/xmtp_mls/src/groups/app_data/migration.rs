//! Errors from building a group's app data dictionary values.
//!
//! The bootstrap bridge that moved legacy groups to app data (sender
//! synthesis and `stage_bootstrap_commit`) is gone. Groups have been created
//! with the dictionary since libxmtp #4152.

use xmtp_mls_common::app_data::migration::MigrationError as CommonMigrationError;

/// Errors from encoding app data dictionary values.
#[derive(Debug, thiserror::Error)]
pub enum BootstrapSynthesisError {
    #[error(transparent)]
    Common(#[from] CommonMigrationError),
}

impl xmtp_common::retry::RetryableError for BootstrapSynthesisError {
    /// Encoding is deterministic over its inputs, so no failure is retryable.
    fn is_retryable(&self) -> bool {
        false
    }
}
