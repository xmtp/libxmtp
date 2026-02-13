use super::{BACKUP_VERSION, OptionsToSave, export_stream::BatchExportStream};
use crate::{NONCE_SIZE, util::GenericArrayExt};
use aes_gcm::{Aes256Gcm, AesGcm, KeyInit, aead::Aead, aes::Aes256};
use async_compression::futures::write::ZstdEncoder;
use futures::{Stream, pin_mut, ready, task::Context};
use futures_util::{AsyncRead, AsyncWriteExt};
use pin_project::pin_project;
use prost::Message;
#[allow(deprecated)]
use sha2::digest::{generic_array::GenericArray, typenum};
use std::{future::Future, io, pin::Pin, sync::Arc, task::Poll};
use xmtp_db::prelude::*;
use xmtp_proto::xmtp::device_sync::{
    BackupElement, BackupMetadataSave, BackupOptions, backup_element::Element,
};

#[cfg(not(target_arch = "wasm32"))]
mod file_export;

#[pin_project]
pub struct ArchiveExporter {
    stage: Stage,
    metadata: BackupMetadataSave,
    #[pin]
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

impl ArchiveExporter {
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn export_to_file<D>(
        options: BackupOptions,
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

    pub async fn post_to_url(self, url: &str) -> Result<String, crate::ArchiveError> {
        #[cfg(not(target_arch = "wasm32"))]
        let body = {
            // 2. A compat layer to have futures AsyncRead play nice with tokio's AsyncRead
            let exporter_compat = tokio_util::compat::FuturesAsyncReadCompatExt::compat(self);
            // 3. Add a stream layer over the async read
            let stream = tokio_util::io::ReaderStream::new(exporter_compat);
            // 4. Pipe that stream as the body to the request to the history server
            reqwest::Body::wrap_stream(stream)
        };
        #[cfg(target_arch = "wasm32")]
        let body = {
            use futures::AsyncReadExt;
            // Make exporter mutable
            let mut exporter = self;

            // Wasm does not support stream uploads. So we'll just consume the stream into a vec.
            let mut buffer = Vec::new();
            exporter.read_to_end(&mut buffer).await?;
            buffer
        };

        tracing::info!("Uploading sync payload to history server...");
        let response = reqwest::Client::new().post(url).body(body).send().await?;
        tracing::info!("Done uploading sync payload to history server.");

        if let Err(err) = response.error_for_status_ref() {
            tracing::error!("Failed to upload file. Status code: {:?}", err.status());
            return Err(crate::ArchiveError::Reqwest(err));
        }

        Ok(response.text().await?)
    }

    pub fn new<D>(options: BackupOptions, db: D, key: &[u8]) -> Self
    where
        D: DbQuery + 'static,
    {
        let mut nonce_buffer = BACKUP_VERSION.to_le_bytes().to_vec();
        let nonce = xmtp_common::rand_array::<NONCE_SIZE>();
        nonce_buffer.extend_from_slice(&nonce);

        Self {
            position: 0,
            stage: Stage::default(),
            stream: BatchExportStream::new(&options, Arc::new(db)),
            metadata: BackupMetadataSave::from_options(options),
            zstd_encoder: ZstdEncoder::new(Vec::new()),
            encoder_finished: false,

            #[allow(deprecated)]
            cipher: Aes256Gcm::new(GenericArray::from_slice(key)),
            #[allow(deprecated)]
            nonce: GenericArray::clone_from_slice(&nonce),
            nonce_buffer,
        }
    }

    pub fn metadata(&self) -> &BackupMetadataSave {
        &self.metadata
    }
}

// The reason this is future_util's AsyncRead and not tokio's AsyncRead
// is because we need this to work on WASM, and tokio's AsyncRead makes
// some assumptions about having access to std::fs, which WASM does not have.
//
// To get around this, we implement AsyncRead using future_util, and use a
// compat layer from tokio_util to be able to interact with it in tokio.
impl AsyncRead for ArchiveExporter {
    /// This function encrypts first, and compresses second.
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        let mut this = self.project();
        loop {
            // Putting this up here because we don't want to encrypt or compress the nonce.
            if matches!(this.stage, Stage::Nonce) {
                let amount = this.nonce_buffer.len().min(buf.len());
                let nonce_bytes: Vec<_> = this.nonce_buffer.drain(..amount).collect();
                buf[..amount].copy_from_slice(&nonce_bytes);

                if this.nonce_buffer.is_empty() {
                    *this.stage = Stage::Metadata;
                }
                return Poll::Ready(Ok(amount));
            }

            {
                // Read from the buffer while there is data
                let buffer_inner = this.zstd_encoder.get_ref();
                if *this.position < buffer_inner.len() {
                    let available = &buffer_inner[*this.position..];
                    let amount = available.len().min(buf.len());
                    buf[..amount].copy_from_slice(&available[..amount]);
                    *this.position += amount;

                    return Poll::Ready(Ok(amount));
                }
            }

            // The buffer is consumed. Reset.
            *this.position = 0;
            this.zstd_encoder.get_mut().clear();

            // Time to fill the buffer with more data 8kb at a time.
            while this.zstd_encoder.get_ref().len() < 8_000 {
                let element = match this.stage {
                    Stage::Nonce => {
                        // Should never get here due to the above logic. Error if it does.
                        unreachable!("Nonce should not be the stage here.");
                    }
                    Stage::Metadata => {
                        *this.stage = Stage::Elements;
                        BackupElement {
                            element: Some(Element::Metadata(this.metadata.clone())),
                        }
                        .encode_to_vec()
                    }
                    Stage::Elements => match ready!(this.stream.as_mut().poll_next(cx)) {
                        Some(element) => element
                            .map_err(|err| io::Error::other(err.to_string()))?
                            .encode_to_vec(),
                        None => {
                            if !*this.encoder_finished {
                                *this.encoder_finished = true;
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
                    .encrypt(this.nonce, &*element)
                    .expect("Encryption should always work");
                let mut bytes = (element.len() as u32).to_le_bytes().to_vec();
                bytes.append(&mut element);
                this.nonce.increment();

                let fut = this.zstd_encoder.write(&bytes);
                pin_mut!(fut);
                match fut.poll(cx) {
                    Poll::Ready(Ok(_amt)) => {}
                    Poll::Ready(Err(err)) => return Poll::Ready(Err(err)),
                    Poll::Pending => return Poll::Pending,
                }
            }

            // Flush the encoder
            if !*this.encoder_finished {
                let fut = this.zstd_encoder.flush();
                pin_mut!(fut);
                let _ = fut.poll(cx)?;
            }

            if this.zstd_encoder.get_ref().is_empty() {
                return Poll::Ready(Ok(0));
            }
        }
    }
}
