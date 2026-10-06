//! Convert closed legacy storage to a standard archive without a network client.
//! The caller closes the legacy SDK and retains the archive key for import.

mod metadata;
mod migrations;
#[cfg(not(target_arch = "wasm32"))]
mod native;
mod records;
#[cfg(target_arch = "wasm32")]
mod wasm;

use xmtp_common::{ErrorCode, RetryableError};

#[cfg(target_arch = "wasm32")]
extern crate uniffi_runtime_wasm as _;

uniffi::setup_scaffolding!();

/// Owned input. Keys are never included in debug output.
#[derive(Clone, uniffi::Record)]
pub struct PrepareMigrationArchiveArgs {
    pub database_path: String,
    pub database_key: Option<Vec<u8>>,
    pub archive_key: Vec<u8>,
    pub output_path: String,
}

/// Counts records written to the completed archive, before import deduplication.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct MigrationReport {
    pub archive_path: String,
    pub group_count: u64,
    pub message_count: u64,
    pub consent_count: u64,
}

/// Stable failure categories. Inner causes remain available to Rust callers.
#[derive(Debug, thiserror::Error, ErrorCode, uniffi::Error)]
#[uniffi(flat_error)]
pub enum MigrationError {
    /// The path, key, or source cannot be used. Not retryable without correction.
    #[error("invalid migration input: {0}")]
    InvalidInput(#[source] InputError),
    /// Another process holds a source lock. Retry after closing that process.
    #[error("legacy storage is busy; close the legacy SDK")]
    SourceBusy,
    /// The migration history is outside the supported range. Not retryable.
    #[error("unsupported legacy schema")]
    UnsupportedSchema,
    /// A pinned database migration failed. Not retryable without correction.
    #[error("legacy database migration failed: {0}")]
    Migration(#[source] diesel::result::Error),
    /// A required record could not be read. Not retryable without correction.
    #[error("legacy record could not be read: {0}")]
    RecordRead(#[source] RecordError),
    /// The archive could not be completed. Retry after correcting the output.
    #[error("migration output failed: {0}")]
    Output(#[source] OutputError),
}

/// Input failures retain their typed causes.
#[derive(Debug, thiserror::Error)]
pub enum InputError {
    #[error("{0}")]
    Invalid(&'static str),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Database(#[from] diesel::ConnectionError),
    #[error(transparent)]
    Read(#[from] diesel::result::Error),
    #[error(transparent)]
    Salt(#[from] hex::FromHexError),
    #[cfg(target_arch = "wasm32")]
    #[error(transparent)]
    Storage(#[from] xmtp_db::StorageError),
}

/// Output failures retain their typed platform causes.
#[derive(Debug, thiserror::Error)]
pub enum OutputError {
    #[error(transparent)]
    Archive(#[from] xmtp_archive::ArchiveError),
    #[cfg(target_arch = "wasm32")]
    #[error("browser archive output failed: {value:?}")]
    Browser { value: wasm_bindgen::JsValue },
}

/// Required row failures are distinct from optional metadata decode failures.
#[derive(Debug, thiserror::Error)]
pub enum RecordError {
    #[error(transparent)]
    Database(#[from] diesel::result::Error),
    #[error("{0}")]
    Invalid(&'static str),
}

impl MigrationError {
    pub(crate) fn invalid(reason: &'static str) -> Self {
        Self::InvalidInput(InputError::Invalid(reason))
    }
    pub(crate) fn migration(source: diesel::result::Error) -> Self {
        Self::Migration(source)
    }
}

impl From<diesel::result::Error> for MigrationError {
    fn from(value: diesel::result::Error) -> Self {
        Self::RecordRead(value.into())
    }
}
impl From<xmtp_archive::ArchiveError> for MigrationError {
    fn from(value: xmtp_archive::ArchiveError) -> Self {
        Self::Output(value.into())
    }
}
impl RetryableError for MigrationError {
    fn is_retryable(&self) -> bool {
        matches!(self, Self::SourceBusy)
    }
}

/// Copies closed legacy storage, migrates that copy, and writes a local archive.
/// The destination is replaced only after all records and the archive footer are
/// complete. Dropping the future cancels work before publication.
#[cfg(not(target_arch = "wasm32"))]
#[uniffi::export(async_runtime = "tokio")]
// implements: MIG-003
pub async fn prepare_migration_archive(
    args: PrepareMigrationArchiveArgs,
) -> Result<MigrationReport, MigrationError> {
    native::prepare(args).await
}

/// Prepares an archive from closed browser storage in the package-owned worker.
#[cfg(target_arch = "wasm32")]
#[uniffi::export]
pub async fn prepare_migration_archive(
    args: PrepareMigrationArchiveArgs,
) -> Result<MigrationReport, MigrationError> {
    wasm::prepare(args).await
}
