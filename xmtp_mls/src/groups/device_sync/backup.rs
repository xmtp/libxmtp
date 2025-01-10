use crate::XmtpOpenMlsProvider;
use backup_stream::{BackupElement, BackupRecordStreamer, BackupStream};
use futures::Stream;
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
        provider: &Arc<XmtpOpenMlsProvider>,
        opts: &BackupOptions,
    ) -> Vec<Pin<Box<dyn Stream<Item = Vec<BackupElement>> + 'a>>> {
        match self {
            Self::Consent => vec![Box::pin(BackupRecordStreamer::<ConsentRecordSave>::new(
                provider, opts,
            ))],
            Self::Messages => vec![],
        }
    }
}

impl BackupOptions {
    pub fn write(self, provider: &Arc<XmtpOpenMlsProvider>) -> BackupStream {
        let input_streams = self
            .elements
            .iter()
            .map(|e| e.to_streamers(provider, &self))
            .collect::<Vec<_>>();

        BackupStream {
            input_streams,
            buffer: vec![],
        }
    }
}
