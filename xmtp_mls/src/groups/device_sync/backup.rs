use xmtp_common::time::now_ns;
use xmtp_proto::xmtp::device_sync::{BackupElementSelection, BackupMetadata};

// Increment on breaking changes
const BACKUP_VERSION: u32 = 0;

mod backup_exporter;
mod backup_importer;
mod backup_stream;

pub struct BackupOptions {
    start_ns: Option<i64>,
    end_ns: Option<i64>,
    elements: Vec<BackupElementSelection>,
}

impl From<BackupOptions> for BackupMetadata {
    fn from(value: BackupOptions) -> Self {
        Self {
            backup_version: BACKUP_VERSION,
            end_ns: value.end_ns,
            start_ns: value.start_ns,
            elements: value.elements.iter().map(|&e| e as i32).collect(),
            exported_at_ns: now_ns(),
        }
    }
}
