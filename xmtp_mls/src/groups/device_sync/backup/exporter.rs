use super::{export_stream::BatchExportStream, OptionsToSave, BACKUP_VERSION};
use crate::{
    groups::device_sync::{DeviceSyncError, NONCE_SIZE},
    XmtpOpenMlsProvider,
};
use aes_gcm::{aead::Aead, aes::Aes256, Aes256Gcm, AesGcm, KeyInit};
use async_compression::futures::write::ZstdEncoder;
use futures::{pin_mut, task::Context, StreamExt};
use futures_util::{AsyncRead, AsyncWriteExt};
use prost::Message;
use sha2::digest::{generic_array::GenericArray, typenum};
use std::{future::Future, io, pin::Pin, sync::Arc, task::Poll};
use xmtp_proto::xmtp::device_sync::{
    backup_element::Element, BackupElement, BackupMetadataSave, BackupOptions,
};

#[cfg(not(target_arch = "wasm32"))]
mod file_export;

pub struct BackupExporter {
    stage: Stage,
    metadata: BackupMetadataSave,
    stream: BatchExportStream,
    position: usize,
    zstd_encoder: ZstdEncoder<Vec<u8>>,
    encoder_finished: bool,

    cipher: AesGcm<Aes256, typenum::U12, typenum::U16>,
    nonce: GenericArray<u8, typenum::U12>,

    // Used to write the nonce, contains the same data as nonce.
    nonce_buffer: Vec<u8>,
}

#[derive(Default)]
pub(super) enum Stage {
    #[default]
    Nonce,
    Metadata,
    Elements,
}

impl BackupExporter {
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn export_to_file(
        options: BackupOptions,
        provider: XmtpOpenMlsProvider,
        path: impl AsRef<std::path::Path>,
        key: &[u8],
    ) -> Result<(), DeviceSyncError> {
        let provider = Arc::new(provider);
        let mut exporter = Self::new(options, &provider, key);
        exporter.write_to_file(path).await?;

        Ok(())
    }

    pub(crate) fn new(
        options: BackupOptions,
        provider: &Arc<XmtpOpenMlsProvider>,
        key: &[u8],
    ) -> Self {
        let mut nonce_buffer = BACKUP_VERSION.to_le_bytes().to_vec();
        let nonce = xmtp_common::rand_array::<NONCE_SIZE>();
        nonce_buffer.extend_from_slice(&nonce);

        Self {
            position: 0,
            stage: Stage::default(),
            stream: BatchExportStream::new(&options, provider),
            metadata: BackupMetadataSave::from_options(options),
            zstd_encoder: ZstdEncoder::new(Vec::new()),
            encoder_finished: false,

            cipher: Aes256Gcm::new(GenericArray::from_slice(key)),
            nonce: GenericArray::clone_from_slice(&nonce),
            nonce_buffer,
        }
    }

    pub(crate) fn metadata(&self) -> &BackupMetadataSave {
        &self.metadata
    }
}

// The reason this is future_util's AsyncRead and not tokio's AsyncRead
// is because we need this to work on WASM, and tokio's AsyncRead makes
// some assumptions about having access to std::fs, which WASM does not have.
//
// To get around this, we implement AsyncRead using future_util, and use a
// compat layer from tokio_util to be able to interact with it in tokio.
impl AsyncRead for BackupExporter {
    /// This function encrypts first, and compresses second.
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();

        // Putting this up here becuase we don't want to encrypt or compress the nonce.
        if matches!(this.stage, Stage::Nonce) {
            let amount = this.nonce_buffer.len().min(buf.len());
            let nonce_bytes: Vec<_> = this.nonce_buffer.drain(..amount).collect();
            buf[..amount].copy_from_slice(&nonce_bytes);

            if this.nonce_buffer.is_empty() {
                this.stage = Stage::Metadata;
            }
            return Poll::Ready(Ok(amount));
        }

        {
            // Read from the buffer while there is data
            let buffer_inner = this.zstd_encoder.get_ref();
            if this.position < buffer_inner.len() {
                let available = &buffer_inner[this.position..];
                let amount = available.len().min(buf.len());
                buf[..amount].copy_from_slice(&available[..amount]);
                this.position += amount;

                return Poll::Ready(Ok(amount));
            }
        }

        // The buffer is consumed. Reset.
        this.position = 0;
        this.zstd_encoder.get_mut().clear();

        // Time to fill the buffer with more data 8kb at a time.
        while this.zstd_encoder.get_ref().len() < 8_000 {
            let element = match this.stage {
                Stage::Nonce => {
                    // Should never get here due to the above logic. Error if it does.
                    unreachable!()
                }
                Stage::Metadata => {
                    this.stage = Stage::Elements;
                    BackupElement {
                        element: Some(Element::Metadata(this.metadata.clone())),
                    }
                    .encode_to_vec()
                }
                Stage::Elements => match this.stream.poll_next_unpin(cx) {
                    Poll::Ready(Some(element)) => element
                        .map_err(|err| io::Error::new(io::ErrorKind::Other, err))?
                        .encode_to_vec(),
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(None) => {
                        if !this.encoder_finished {
                            this.encoder_finished = true;
                            let fut = this.zstd_encoder.close();
                            pin_mut!(fut);
                            let _ = fut.poll(cx)?;
                        }
                        break;
                    }
                },
            };

            let mut element = this
                .cipher
                .encrypt(&this.nonce, &*element)
                .expect("Encryption should always work");
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
            Poll::Ready(Ok(0))
        } else {
            Pin::new(&mut *this).poll_read(cx, buf)
        }
    }
}
