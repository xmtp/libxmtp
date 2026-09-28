use super::{BACKUP_VERSION, OptionsToSave, export_stream::BatchExportStream};
use crate::archive_options::ArchiveOptions;
use crate::{NONCE_SIZE, util::GenericArrayExt};
use aes_gcm::{Aes256Gcm, AesGcm, KeyInit, aead::Aead, aes::Aes256};
use async_compression::futures::write::ZstdEncoder;
use futures::{Stream, ready, task::Context};
use futures_util::{AsyncRead, AsyncWrite};
use pin_project::pin_project;
use prost::Message;
#[allow(deprecated)]
use sha2::digest::{generic_array::GenericArray, typenum};
use std::{io, pin::Pin, sync::Arc, task::Poll};
use xmtp_db::prelude::*;
use xmtp_proto::xmtp::device_sync::{BackupElement, BackupMetadataSave, backup_element::Element};

#[cfg(not(target_arch = "wasm32"))]
mod file_export;

const OUTPUT_BATCH_SIZE: usize = 8_000;

#[pin_project]
pub struct ArchiveExporter {
    metadata: BackupMetadataSave,
    #[pin]
    reader: ArchiveReader<ZstdEncoder<Vec<u8>>>,
}

#[pin_project]
struct ArchiveReader<E> {
    stage: Stage,
    #[pin]
    stream: BatchExportStream,
    position: usize,
    encoder: E,
    frame: Vec<u8>,
    frame_position: usize,
    cipher: AesGcm<Aes256, typenum::U12, typenum::U16>,
    nonce: GenericArray<u8, typenum::U12>,
    nonce_buffer: Vec<u8>,
}

#[derive(Default)]
enum Stage {
    #[default]
    Nonce,
    Metadata,
    Elements,
    Flushing,
    Closing,
    Finished,
}

// Keep the output buffer accessible without coupling progress to zstd.
trait BufferedEncoder: AsyncWrite + Unpin {
    fn output(&mut self) -> &mut Vec<u8>;
}

impl BufferedEncoder for ZstdEncoder<Vec<u8>> {
    fn output(&mut self) -> &mut Vec<u8> {
        self.get_mut()
    }
}

impl ArchiveExporter {
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn export_to_file<D>(
        options: ArchiveOptions,
        db: D,
        path: impl AsRef<std::path::Path>,
        key: &[u8],
    ) -> Result<BackupMetadataSave, crate::ArchiveError>
    where
        D: DbQuery + 'static,
    {
        let mut exporter = Self::new(options, db, key);
        exporter.write_to_file(path).await?;

        Ok(exporter.metadata)
    }

    pub fn new<D>(options: ArchiveOptions, db: D, key: &[u8]) -> Self
    where
        D: DbQuery + 'static,
    {
        let mut nonce_buffer = BACKUP_VERSION.to_le_bytes().to_vec();
        let nonce = xmtp_common::rand_array::<NONCE_SIZE>();
        nonce_buffer.extend_from_slice(&nonce);

        let stream = BatchExportStream::new(&options, Arc::new(db));
        Self {
            metadata: BackupMetadataSave::from_options(options),
            reader: ArchiveReader {
                position: 0,
                stage: Stage::default(),
                stream,
                encoder: ZstdEncoder::new(Vec::new()),
                frame: Vec::new(),
                frame_position: 0,
                #[allow(deprecated)]
                cipher: Aes256Gcm::new(GenericArray::from_slice(key)),
                #[allow(deprecated)]
                nonce: GenericArray::clone_from_slice(&nonce),
                nonce_buffer,
            },
        }
    }

    pub fn metadata(&self) -> &BackupMetadataSave {
        &self.metadata
    }
}

// futures_util supports the same reader on native and WASM targets.
impl AsyncRead for ArchiveExporter {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.project();
        this.reader.poll_read(cx, buf, this.metadata)
    }
}

impl<E: BufferedEncoder> ArchiveReader<E> {
    // implements: ARCH-001, ARCH-017
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
        metadata: &BackupMetadataSave,
    ) -> Poll<io::Result<usize>> {
        if buf.is_empty() {
            return Poll::Ready(Ok(0));
        }
        let mut this = self.project();
        loop {
            if matches!(this.stage, Stage::Nonce) {
                let amount = this.nonce_buffer.len().min(buf.len());
                buf[..amount].copy_from_slice(&this.nonce_buffer[..amount]);
                this.nonce_buffer.drain(..amount);
                if this.nonce_buffer.is_empty() {
                    *this.stage = Stage::Metadata;
                }
                return Poll::Ready(Ok(amount));
            }

            let output = this.encoder.output();
            if *this.position < output.len() {
                let amount = (output.len() - *this.position).min(buf.len());
                buf[..amount].copy_from_slice(&output[*this.position..*this.position + amount]);
                *this.position += amount;
                return Poll::Ready(Ok(amount));
            }
            *this.position = 0;
            output.clear();

            loop {
                // Do not poll another element or advance the nonce until this frame is accepted.
                if !this.frame.is_empty() {
                    while *this.frame_position < this.frame.len() {
                        let amount = ready!(
                            Pin::new(&mut *this.encoder)
                                .poll_write(cx, &this.frame[*this.frame_position..])
                        )?;
                        if amount == 0 {
                            return Poll::Ready(Err(io::ErrorKind::WriteZero.into()));
                        }
                        *this.frame_position += amount;
                    }
                    this.frame.clear();
                    *this.frame_position = 0;
                    this.nonce.increment();
                    if matches!(this.stage, Stage::Metadata) {
                        *this.stage = Stage::Elements;
                    }
                }

                if matches!(this.stage, Stage::Elements)
                    && this.encoder.output().len() >= OUTPUT_BATCH_SIZE
                {
                    *this.stage = Stage::Flushing;
                }

                let element = match this.stage {
                    Stage::Nonce => unreachable!("header is written before frames"),
                    Stage::Metadata => BackupElement {
                        element: Some(Element::Metadata(metadata.clone())),
                    },
                    Stage::Elements => match ready!(this.stream.as_mut().poll_next(cx)) {
                        Some(element) => element.map_err(storage_io_error)?,
                        None => {
                            *this.stage = Stage::Closing;
                            continue;
                        }
                    },
                    Stage::Flushing => {
                        ready!(Pin::new(&mut *this.encoder).poll_flush(cx))?;
                        *this.stage = Stage::Elements;
                        break;
                    }
                    Stage::Closing => {
                        ready!(Pin::new(&mut *this.encoder).poll_close(cx))?;
                        *this.stage = Stage::Finished;
                        break;
                    }
                    Stage::Finished => return Poll::Ready(Ok(0)),
                };
                let encrypted = this
                    .cipher
                    .encrypt(this.nonce, element.encode_to_vec().as_slice())
                    .map_err(io::Error::other)?;
                this.frame
                    .extend_from_slice(&(encrypted.len() as u32).to_le_bytes());
                this.frame.extend_from_slice(&encrypted);
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn storage_io_error(error: xmtp_db::StorageError) -> io::Error {
    io::Error::other(error)
}

#[cfg(target_arch = "wasm32")]
fn storage_io_error(error: xmtp_db::StorageError) -> io::Error {
    // I/O errors require Send + Sync sources, but WASM storage errors can
    // contain local sources. Keep their message; native keeps the typed cause.
    io::Error::other(error.to_string())
}

#[cfg(test)]
mod tests;
