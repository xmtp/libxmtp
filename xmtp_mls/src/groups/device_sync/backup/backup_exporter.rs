use super::{backup_stream::BackupStream, BackupOptions};
use crate::{groups::device_sync::DeviceSyncError, XmtpOpenMlsProvider};
use futures::StreamExt;
use prost::Message;
use std::{path::Path, pin::Pin, sync::Arc, task::Poll};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt, ReadBuf};
use xmtp_proto::xmtp::device_sync::BackupMetadata;

pub(super) struct BackupExporter {
    stage: Stage,
    metadata: BackupMetadata,
    stream: BackupStream,
    buffer: Option<Vec<u8>>,
    position: usize,
}

#[derive(Default)]
pub(super) enum Stage {
    #[default]
    Metadata,
    Elements,
}

impl BackupExporter {
    pub(super) fn new(opts: BackupOptions, provider: &Arc<XmtpOpenMlsProvider>) -> Self {
        Self {
            buffer: None,
            position: 0,
            stage: Stage::default(),
            stream: BackupStream::new(&opts, provider),
            metadata: opts.into(),
        }
    }

    pub async fn write_to_file(&mut self, path: impl AsRef<Path>) -> Result<(), DeviceSyncError> {
        let mut file = tokio::fs::File::create(path.as_ref()).await?;
        let mut buffer = [0u8; 1024];
        let mut read_buf = tokio::io::ReadBuf::new(&mut buffer);

        while self.read_buf(&mut read_buf).await? != 0 {
            file.write_all(read_buf.filled()).await?;
            read_buf.clear();
        }

        file.flush().await?;

        Ok(())
    }
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
