use crate::XmtpOpenMlsProvider;
use backup_stream::{BackupRecordStreamer, BackupStream};
use futures::{Stream, StreamExt};
use prost::Message;
use std::{pin::Pin, sync::Arc, task::Poll};
use tokio::io::{AsyncRead, ReadBuf};
use xmtp_common::time::now_ns;
use xmtp_proto::xmtp::device_sync::{
    consent_backup::ConsentSave, BackupElement, BackupElementSelection, BackupMetadata,
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

impl BackupOptionsElementSelection {
    fn to_streamers<'a>(
        &self,
        provider: &Arc<XmtpOpenMlsProvider>,
        opts: &BackupOptions,
    ) -> Vec<Pin<Box<dyn Stream<Item = Vec<BackupElement>> + 'a>>> {
        match self {
            Self::Consent => vec![Box::pin(BackupRecordStreamer::<ConsentSave>::new(
                provider, opts,
            ))],
            Self::Messages => vec![],
        }
    }
}

struct BackupWriter {
    stage: Stage,
    metadata: Vec<u8>,
    stream: BackupStream,
    buffer: Option<Vec<u8>>,
    position: usize,
}

#[derive(Default)]
enum Stage {
    #[default]
    MetadataLen,
    Metadata,
    Elements,
}

impl AsyncRead for BackupWriter {
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
            Stage::MetadataLen => {
                buffer.put_slice(&(self.metadata.len() as u32).to_le_bytes());
            }
            Stage::Metadata => {
                buffer.put_slice(&serde_json::to_vec(&self.metadata)?);
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
