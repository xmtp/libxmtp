//! Encrypted XMTP history archives.
//!
//! [`exporter::ArchiveExporter`] reads one consistent database snapshot when it is
//! constructed (see `snapshot`), then streams it as an encrypted, compressed
//! archive. Construction fails with [`ArchiveError`] before any byte is written if
//! selected data cannot be read. [`ArchiveImporter`] decrypts and decodes an archive
//! into [`xmtp_proto::xmtp::device_sync::BackupElement`]s for the caller to restore.
//! [`archive_options::ArchiveOptions`] selects the elements and message window.

use crate::archive_options::BackupElementSelection;
pub use importer::ArchiveImporter;
use thiserror::Error;
use xmtp_db::{ConnectionError, StorageError, diesel, sql_key_store::SqlKeyStoreError};
use xmtp_mls_common::group_metadata::GroupMetadataError;
use xmtp_proto::{types::GroupId, xmtp::device_sync::BackupMetadataSave};

pub const ENC_KEY_SIZE: usize = 32; // 256-bit key
pub const NONCE_SIZE: usize = 12; // 96-bit nonce

// Increment on breaking changes
pub const BACKUP_VERSION: u16 = 0;

pub mod archive_options;
pub mod exporter;
pub mod importer;
mod snapshot;
mod util;

#[derive(Debug, Error)]
pub enum ArchiveError {
    #[error("Missing metadata")]
    MissingMetadata,
    #[error("Invalid archive frame: {0}")]
    InvalidFrame(&'static str),
    #[error("AES-GCM encryption error")]
    AesGcm(#[from] aes_gcm::Error),
    #[error("IO error: {0}")]
    IO(#[from] std::io::Error),
    #[error(transparent)]
    Decode(#[from] prost::DecodeError),
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error("group {group_id} cannot be exported: {source}")]
    UnreadableGroup {
        group_id: GroupId,
        #[source]
        source: UnreadableGroup,
    },
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

    pub fn from_metadata_version_unknown(save: BackupMetadataSave) -> Self {
        Self::from_metadata_save(save, u16::MAX)
    }
}
