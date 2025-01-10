use crate::storage::DbConnection;
use backup_stream::{BackupElement, BackupRecordStreamer, BackupStream};
use futures::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::{pin::Pin, sync::Arc};
use xmtp_proto::xmtp::device_sync::consent_backup::ConsentRecordSave;

mod backup_stream;

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
    fn to_streamers<'a>(
        &self,
        conn: &'a DbConnection,
        opts: &BackupOptions,
    ) -> Vec<Pin<Box<dyn Stream<Item = Vec<BackupElement>> + 'a>>> {
        match self {
            Self::Consent => vec![Box::pin(BackupRecordStreamer::<ConsentRecordSave>::new(
                conn, opts,
            ))],
            Self::Messages => vec![],
        }
    }
}

impl BackupOptions {
    pub fn write(self, conn: &'static DbConnection) -> BackupStream {
        let input_streams = self
            .elements
            .iter()
            .map(|e| e.to_streamers(conn, &self))
            .collect::<Vec<_>>();

        BackupStream {
            input_streams,
            buffer: vec![],
        }
    }
}
