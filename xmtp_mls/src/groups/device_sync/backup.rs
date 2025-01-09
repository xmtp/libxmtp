use crate::storage::DbConnection;
use backup_element::{BackupElement, BackupRecordStreamer};
use futures::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::{pin::Pin, sync::Arc};
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
    ) -> Vec<Pin<Box<dyn Stream<Item = Vec<BackupElement>>>>> {
        match self {
            Self::Consent => vec![Box::pin(BackupRecordStreamer::<ConsentRecordSave>::new(
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
            buffer: vec![],
        }
    }
}

struct BackupWriter {
    buffer: Vec<BackupElement>,
    options: BackupOptions,
    input_streams: Vec<Vec<Pin<Box<dyn Stream<Item = Vec<BackupElement>>>>>>,
}

impl Stream for BackupWriter {
    type Item = BackupElement;
    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        use std::task::Poll;

        let this = self.get_mut();

        if let Some(element) = this.buffer.pop() {
            return Poll::Ready(Some(element));
        }

        let element = loop {
            let Some(last) = this.input_streams.last_mut() else {
                // No streams left, we're done.
                return Poll::Ready(None);
            };
            if let Some(last) = last.last_mut() {
                let v = match last.as_mut().poll_next(cx) {
                    Poll::Ready(v) => v,
                    Poll::Pending => return Poll::Pending,
                };
                if let Some(v) = v {
                    this.buffer = v;
                    if let Some(element) = this.buffer.pop() {
                        break element;
                    }
                }
            };

            this.input_streams.pop();
        };

        Poll::Ready(Some(element))
    }
}
