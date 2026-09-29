//! The XMTP archive container: a versioned header, then AES-256-GCM frames of `BackupElement`s
//! inside one zstd stream.
//!
//! `xmtp_mls`, `xmtp_sdk`, and the bindings use [`exporter::export`], or the
//! native `exporter::ArchiveExporter` adapters, which write a file or a buffer off
//! the async workers, to write an archive from the database and [`ArchiveImporter`] to
//! read one back. [`check_key`] validates a key up front. [`ArchiveError`]
//! implements `RetryableError`. The exporter streams one consistent snapshot of every selected element (see
//! `snapshot`) and fails with [`ArchiveError`] if selected data cannot be read. The importer rejects
//! any container version above [`BACKUP_VERSION`] and ends with an error on any incomplete or
//! malformed framing.
use crate::archive_options::BackupElementSelection;
use aes_gcm::{Aes256Gcm, KeyInit};
pub use importer::ArchiveImporter;
use thiserror::Error;
use xmtp_common::{ErrorCode, RetryableError};
use xmtp_db::{ConnectionError, StorageError, diesel, sql_key_store::SqlKeyStoreError};
use xmtp_mls_common::{
    group_metadata::GroupMetadataError, group_mutable_metadata::GroupMutableMetadataError,
};
use xmtp_proto::{types::GroupId, xmtp::device_sync::BackupMetadataSave};

pub const ENC_KEY_SIZE: usize = 32; // 256-bit key
pub const NONCE_SIZE: usize = 12; // 96-bit nonce
pub const TAG_SIZE: usize = 16; // 128-bit AES-GCM tag

// Increment on breaking changes
pub const BACKUP_VERSION: u16 = 0;

pub mod archive_options;
pub mod exporter;
pub mod importer;
mod snapshot;
mod util;

/// Archive export or import failure.
#[derive(Debug, Error, ErrorCode)]
pub enum ArchiveError {
    /// Unsupported archive version.
    ///
    /// The archive was written by a newer client. Not retryable.
    #[error("Unsupported archive version {0}; this client reads version {BACKUP_VERSION}")]
    UnsupportedVersion(u16),
    /// Missing metadata.
    ///
    /// The archive has no metadata frame. Not retryable.
    #[error("Missing metadata")]
    MissingMetadata,
    /// Invalid archive frame.
    ///
    /// The archive framing is incomplete or malformed. Not retryable.
    #[error("Invalid archive frame: {0}")]
    InvalidFrame(&'static str),
    /// AES-GCM error.
    ///
    /// Encryption or decryption failed; on import, usually a wrong key. Not retryable.
    #[error("AES-GCM encryption error")]
    AesGcm(#[from] aes_gcm::Error),
    /// I/O error.
    ///
    /// Reading or writing the archive failed. May be retryable.
    #[error("IO error: {0}")]
    IO(#[from] std::io::Error),
    /// Decode error.
    ///
    /// An archive element is not valid protobuf. Not retryable.
    #[error(transparent)]
    Decode(#[from] prost::DecodeError),
    #[error(transparent)]
    #[error_code(inherit)]
    Storage(#[from] StorageError),
    /// Invalid key length.
    ///
    /// The archive key is not [`ENC_KEY_SIZE`] bytes. Rejected before any archive byte is
    /// read or written. Not retryable.
    #[error("archive key must be {ENC_KEY_SIZE} bytes, got {0}")]
    InvalidKeyLength(usize),
    /// Unreadable group.
    ///
    /// A selected group's MLS state or metadata cannot be read, so export
    /// fails rather than omit it. Retryable only when reading its MLS state
    /// failed transiently.
    #[error("group {group_id} cannot be exported: {source}")]
    UnreadableGroup {
        group_id: GroupId,
        #[source]
        source: UnreadableGroup,
    },
}

impl RetryableError for ArchiveError {
    /// Only interrupted or timed-out I/O and transient storage failures,
    /// including those reading a group's MLS state. Every other I/O kind, such
    /// as the `UnexpectedEof` of a truncated archive, and every format, key,
    /// missing-state and metadata error recurs on each attempt.
    fn is_retryable(&self) -> bool {
        use std::io::ErrorKind::*;
        match self {
            Self::IO(e) => matches!(e.kind(), Interrupted | WouldBlock | TimedOut),
            Self::Storage(e) => e.is_retryable(),
            Self::UnreadableGroup {
                source: UnreadableGroup::State(e),
                ..
            } => e.is_retryable(),
            _ => false,
        }
    }
}

impl From<ConnectionError> for ArchiveError {
    fn from(e: ConnectionError) -> Self {
        Self::Storage(e.into())
    }
}

impl From<diesel::result::Error> for ArchiveError {
    fn from(e: diesel::result::Error) -> Self {
        Self::Storage(e.into())
    }
}

/// Why an eligible group could not be exported.
#[derive(Debug, Error)]
pub enum UnreadableGroup {
    #[error("no MLS group state")]
    MissingState,
    #[error(transparent)]
    State(#[from] SqlKeyStoreError),
    #[error(transparent)]
    Metadata(#[from] GroupMetadataError),
    #[error(transparent)]
    MutableMetadata(#[from] GroupMutableMetadataError),
}

/// Rejects a key that is not [`ENC_KEY_SIZE`] bytes. Export and import call
/// it first; callers holding a large input may call it before copying that
/// input.
// implements: ARCH-012
pub fn check_key(key: &[u8]) -> Result<(), ArchiveError> {
    match key.len() {
        ENC_KEY_SIZE => Ok(()),
        len => Err(ArchiveError::InvalidKeyLength(len)),
    }
}

/// The archive cipher for `key`, which must be [`ENC_KEY_SIZE`] bytes.
fn cipher(key: &[u8]) -> Result<Aes256Gcm, ArchiveError> {
    Aes256Gcm::new_from_slice(key).map_err(|_| ArchiveError::InvalidKeyLength(key.len()))
}

#[derive(Default)]
pub struct BackupMetadata {
    pub backup_version: u16,
    pub elements: Vec<BackupElementSelection>,
    pub exported_at_ns: i64,
    pub start_ns: Option<i64>,
    pub end_ns: Option<i64>,
}

impl BackupMetadata {
    pub fn from_metadata_save(save: BackupMetadataSave, backup_version: u16) -> Self {
        Self {
            elements: save.elements().map(Into::into).collect(),
            end_ns: save.end_ns,
            start_ns: save.start_ns,
            exported_at_ns: save.exported_at_ns,
            backup_version,
        }
    }
}
