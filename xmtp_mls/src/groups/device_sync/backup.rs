use crate::storage::DbConnection;
use backup_element::{BackupElement, BackupRecordStreamer};
use futures::Stream;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use xmtp_proto::xmtp::device_sync::consent_backup::ConsentRecordSave;

mod backup_element;

#[derive(Serialize, Deserialize)]
pub struct BackupMetadata {
    exported_at_ns: u64,
    exported_elements: Vec<BackupOptionsElementSelection>,
    /// Range of timestamp messages from_ns..to_ns
    start_ns: Option<u64>,
    end_ns: Option<u64>,
}

pub struct BackupOptions {
    start_ns: Option<u64>,
    end_ns: Option<u64>,
    elements: Vec<BackupOptionsElementSelection>,
}

#[derive(Serialize, Deserialize, Clone)]
pub enum BackupOptionsElementSelection {
    Messages,
    Consent,
}

impl BackupOptionsElementSelection {
    fn to_streamers(
        &self,
        conn: &Arc<DbConnection>,
        opts: &BackupOptions,
    ) -> Vec<Box<dyn Stream<Item = Vec<BackupElement>>>> {
        match self {
            Self::Consent => vec![Box::new(BackupRecordStreamer::<ConsentRecordSave>::new(
                conn, opts,
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
            .map(|e| e.to_streamers(conn, &self))
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
