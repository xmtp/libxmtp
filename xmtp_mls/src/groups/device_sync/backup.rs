use crate::XmtpOpenMlsProvider;
use backup_stream::{BackupRecordStreamer, BackupStream};
use futures::StreamExt;
use prost::Message;
use std::{pin::Pin, sync::Arc, task::Poll};
use tokio::io::{AsyncRead, ReadBuf};
use xmtp_common::time::now_ns;
use xmtp_proto::xmtp::device_sync::{
    consent_backup::ConsentSave, group_backup::GroupSave, message_backup::GroupMessageSave,
    BackupElementSelection, BackupMetadata,
};

const BACKUP_VERSION: u32 = 0;

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

impl BackupOptions {
    pub fn export(self, provider: &Arc<XmtpOpenMlsProvider>) -> BackupExporter {
        BackupExporter {
            buffer: None,
            position: 0,
            stage: Stage::default(),
            stream: self.build_stream(provider),
            metadata: self.into(),
        }
    }

    fn build_stream(&self, provider: &Arc<XmtpOpenMlsProvider>) -> BackupStream {
        use BackupElementSelection::*;
        let input_streams = self
            .elements
            .iter()
            .flat_map(|&e| match e {
                Consent => vec![BackupRecordStreamer::<ConsentSave>::new(provider, self)],
                Messages => vec![
                    BackupRecordStreamer::<GroupSave>::new(provider, self),
                    BackupRecordStreamer::<GroupMessageSave>::new(provider, self),
                ],
            })
            .collect();

        BackupStream {
            input_streams,
            buffer: vec![],
        }
    }
}

struct BackupExporter {
    stage: Stage,
    metadata: BackupMetadata,
    stream: BackupStream,
    buffer: Option<Vec<u8>>,
    position: usize,
}

#[derive(Default)]
enum Stage {
    #[default]
    Metadata,
    Elements,
}

impl AsyncRead for BackupExporter {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let mut buffer_inner = self.buffer.take().unwrap_or_default();
        if self.position < buffer_inner.len() {
            let available = &buffer_inner[self.position..];
            let amount = available.len().min(buf.remaining());
            buf.put_slice(&available[..amount]);
            self.position += amount;
            self.buffer = Some(buffer_inner);
            return Poll::Ready(Ok(()));
        }

        // The buffer is consumed. Reset.
        self.position = 0;
        buffer_inner.clear();

        // Time to fill the buffer with more data.
        let mut buffer = ReadBuf::new(&mut buffer_inner);

        match self.stage {
            Stage::Metadata => {
                buffer.put_slice(&serde_json::to_vec(&self.metadata)?);
                self.stage = Stage::Elements;
            }
            Stage::Elements => match self.stream.poll_next_unpin(cx) {
                Poll::Ready(Some(element)) => {
                    element.encode(&mut buffer)?;
                }
                Poll::Ready(None) => {}
                Poll::Pending => {
                    return Poll::Pending;
                }
            },
        };

        let filled = buffer.filled();
        let amount = filled.len().min(buf.remaining());
        buf.put_slice(&filled[..amount]);
        self.position = amount;

        self.buffer = Some(buffer_inner);

        Poll::Ready(Ok(()))
    }
}
