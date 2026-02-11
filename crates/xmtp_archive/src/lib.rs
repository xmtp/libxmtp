pub use importer::ArchiveImporter;
use thiserror::Error;
use xmtp_common::time::now_ns;
use xmtp_proto::xmtp::device_sync::{BackupElementSelection, BackupMetadataSave, ArchiveOptions};

pub const ENC_KEY_SIZE: usize = 32; // 256-bit key
pub const NONCE_SIZE: usize = 12; // 96-bit nonce

// Increment on breaking changes
pub const BACKUP_VERSION: u16 = 0;

mod export_stream;
pub mod exporter;
pub mod importer;
mod util;

#[derive(Debug, Error)]
pub enum ArchiveError {
    #[error("Missing metadata")]
    MissingMetadata,
    #[error("AES-GCM encryption error")]
    AesGcm(#[from] aes_gcm::Error),
    #[error("IO error: {0}")]
    IO(#[from] std::io::Error),
    #[error(transparent)]
    Decode(#[from] prost::DecodeError),
    #[error("reqwest error: {0}")]
    Reqwest(#[from] reqwest::Error),
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
            elements: save.elements().collect(),
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

pub(crate) trait OptionsToSave {
    fn from_options(options: ArchiveOptions) -> BackupMetadataSave;
}
impl OptionsToSave for BackupMetadataSave {
    fn from_options(options: ArchiveOptions) -> BackupMetadataSave {
        Self {
            end_ns: options.end_ns,
            start_ns: options.start_ns,
            elements: options.elements,
            exported_at_ns: now_ns(),
        }
    }
}
