use crate::storage::DbConnection;
use backup_element::{BackupElement, BackupRecordStreamer};
use futures::Stream;
use serde::{Deserialize, Serialize};
use std::{ops::Range, sync::Arc};
use xmtp_proto::xmtp::device_sync::consent_backup::ConsentRecordSave;

mod backup_element;

#[derive(Serialize, Deserialize)]
pub struct BackupMetadata {
    exported_at_ns: u64,
    exported_elements: Vec<BackupSelection>,
    /// Range of timestamp messages from_ns..to_ns
    from_ns: u64,
    to_ns: u64,
}

pub struct BackupOptions {
    range_ns: Option<Range<u64>>,
    elements: Vec<BackupSelection>,
}

#[derive(Serialize, Deserialize, Clone)]
pub enum BackupSelection {
    Messages,
    Consent,
}

impl BackupSelection {
    fn to_streamers(
        &self,
        conn: &Arc<DbConnection>,
    ) -> Vec<Box<dyn Stream<Item = Vec<BackupElement>>>> {
        match self {
            Self::Consent => vec![Box::new(BackupRecordStreamer::<ConsentRecordSave>::new(
                conn,
            ))],
            Self::Messages => vec![],
        }
    }
}

impl BackupOptions {
    pub fn write(self, conn: &Arc<DbConnection>) -> BackupWriter {
        let input_streams = self
            .elements
            .iter()
            .map(|e| e.to_streamers(conn))
            .collect::<Vec<_>>();

        BackupWriter {
            options: self,
            input_streams,
        }
    }
}

struct BackupWriter {
    options: BackupOptions,
    input_streams: Vec<Vec<Box<dyn Stream<Item = Vec<BackupElement>>>>>,
}
