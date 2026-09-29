//! The XMTP archive container: a versioned header, then AES-256-GCM frames of `BackupElement`s
//! inside one zstd stream.
//!
//! `xmtp_mls` and `xmtp_sdk` use [`exporter::ArchiveExporter`] to write an archive from the
//! database and [`ArchiveImporter`] to read one back. The importer rejects any container version
//! above [`BACKUP_VERSION`] and ends with an error on any incomplete or malformed framing.
use crate::archive_options::{ArchiveOptions, BackupElementSelection};
pub use importer::ArchiveImporter;
use thiserror::Error;
use xmtp_common::time::now_ns;
use xmtp_proto::xmtp::device_sync::{
    BackupElementSelection as BackupElementSelectionProto, BackupMetadataSave,
};

pub const ENC_KEY_SIZE: usize = 32; // 256-bit key
pub const NONCE_SIZE: usize = 12; // 96-bit nonce
pub const TAG_SIZE: usize = 16; // 128-bit AES-GCM tag

// Increment on breaking changes
pub const BACKUP_VERSION: u16 = 0;

pub mod archive_options;
mod export_stream;
pub mod exporter;
pub mod importer;
mod util;

#[derive(Debug, Error)]
pub enum ArchiveError {
    #[error("Unsupported archive version {0}; this client reads version {BACKUP_VERSION}")]
    UnsupportedVersion(u16),
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

pub(crate) trait OptionsToSave {
    fn from_options(options: ArchiveOptions) -> BackupMetadataSave;
}
impl OptionsToSave for BackupMetadataSave {
    fn from_options(options: ArchiveOptions) -> BackupMetadataSave {
        Self {
            end_ns: options.end_ns,
            start_ns: options.start_ns,
            elements: options
                .elements
                .into_iter()
                .map(|e| {
                    let e: BackupElementSelectionProto = e.into();
                    e as i32
                })
                .collect(),
            exported_at_ns: now_ns(),
        }
    }
}
