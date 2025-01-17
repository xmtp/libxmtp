use super::{export_stream::BatchExportStream, BackupOptions};
use crate::{groups::device_sync::DeviceSyncError, XmtpOpenMlsProvider};
use async_compression::tokio::write::ZstdEncoder;
use futures::{pin_mut, StreamExt};
use prost::Message;
use std::{future::Future, path::Path, pin::Pin, sync::Arc, task::Poll};
use tokio::{
    fs::File,
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
};
use xmtp_proto::xmtp::device_sync::{backup_element::Element, BackupElement, BackupMetadata};

pub(super) struct BackupExporter {
    stage: Stage,
    metadata: BackupMetadata,
    stream: BatchExportStream,
    position: usize,
    zstd_encoder: ZstdEncoder<Vec<u8>>,
    encoder_finished: bool,
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
            position: 0,
            stage: Stage::default(),
            stream: BatchExportStream::new(&opts, provider),
            metadata: opts.into(),
            zstd_encoder: ZstdEncoder::new(Vec::new()),
            encoder_finished: false,
        }
    }

    pub async fn write_to_file(&mut self, path: impl AsRef<Path>) -> Result<(), DeviceSyncError> {
        let mut file = File::create(path.as_ref()).await?;
        let mut buffer = [0u8; 1024];

        let mut amount = self.read(&mut buffer).await?;
        while amount != 0 {
            file.write(&buffer[..amount]).await?;
            amount = self.read(&mut buffer).await?;
        }

        file.flush().await?;

        Ok(())
    }
}

impl AsyncRead for BackupExporter {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let this = self.get_mut();

        {
            // Read from the buffer while there is data
            let buffer_inner = this.zstd_encoder.get_ref();
            if this.position < buffer_inner.len() {
                let available = &buffer_inner[this.position..];
                let amount = available.len().min(buf.remaining());
                buf.put_slice(&available[..amount]);
                this.position += amount;

                return Poll::Ready(Ok(()));
            }
        }

        // The buffer is consumed. Reset.
        this.position = 0;
        this.zstd_encoder.get_mut().clear();

        // Time to fill the buffer with more data 8kb at a time.
        while this.zstd_encoder.get_ref().len() < 8_000 {
            let mut element = match this.stage {
                Stage::Metadata => {
                    this.stage = Stage::Elements;
                    BackupElement {
                        element: Some(Element::Metadata(this.metadata.clone())),
                    }
                    .encode_to_vec()
                }
                Stage::Elements => match this.stream.poll_next_unpin(cx) {
                    Poll::Ready(Some(element)) => element.encode_to_vec(),
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(None) => {
                        if !this.encoder_finished {
                            this.encoder_finished = true;
                            let fut = this.zstd_encoder.shutdown();
                            pin_mut!(fut);
                            let _ = fut.poll(cx)?;
                        }
                        break;
                    }
                },
            };

            let mut bytes = (element.len() as u32).to_le_bytes().to_vec();
            bytes.append(&mut element);
            let fut = this.zstd_encoder.write(&bytes);
            pin_mut!(fut);
            match fut.poll(cx) {
                Poll::Ready(Ok(_amt)) => {}
                Poll::Ready(Err(err)) => return Poll::Ready(Err(err)),
                Poll::Pending => return Poll::Pending,
            }
        }

        // Flush the encoder
        if !this.encoder_finished {
            let fut = this.zstd_encoder.flush();
            pin_mut!(fut);
            let _ = fut.poll(cx)?;
        }

        if this.zstd_encoder.get_ref().is_empty() {
            Poll::Ready(Ok(()))
        } else {
            Pin::new(&mut *this).poll_read(cx, buf)
        }
    }
}
