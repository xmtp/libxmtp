use super::{ArchiveError, BackupMetadata};
use crate::{NONCE_SIZE, util::GenericArrayExt};
use aes_gcm::{Aes256Gcm, AesGcm, KeyInit, aead::Aead, aes::Aes256};
use async_compression::futures::bufread::ZstdDecoder;
use futures::{FutureExt, Stream, StreamExt};
use futures_util::{AsyncBufRead, AsyncReadExt};
use prost::Message;
#[allow(deprecated)]
use sha2::digest::{generic_array::GenericArray, typenum};
use std::{pin::Pin, task::Poll};
use xmtp_common::{if_native, if_wasm};
use xmtp_proto::xmtp::device_sync::{BackupElement, backup_element::Element};

if_native! {
    mod file_import;
    type AsyncReader = Pin<Box<dyn AsyncBufRead + Send>>;
}
if_wasm! {
    type AsyncReader = Pin<Box<dyn AsyncBufRead>>;
}

pub struct ArchiveImporter {
    pub metadata: BackupMetadata,
    decoded: Vec<u8>,
    decoder: ZstdDecoder<AsyncReader>,

    cipher: AesGcm<Aes256, typenum::U12, typenum::U16>,
    #[allow(deprecated)]
    nonce: GenericArray<u8, typenum::U12>,
}

impl Stream for ArchiveImporter {
    type Item = Result<BackupElement, ArchiveError>;

    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let this = self.get_mut();

        let mut buffer = [0u8; 1024];
        let mut element_len = 0;
        loop {
            let amount = match this.decoder.read(&mut buffer).poll_unpin(cx) {
                Poll::Ready(Ok(amt)) => amt,
                Poll::Ready(Err(err)) => return Poll::Ready(Err(err)?),
                Poll::Pending => return Poll::Pending,
            };
            this.decoded.extend_from_slice(&buffer[..amount]);

            if element_len == 0 && this.decoded.len() >= 4 {
                let bytes = this.decoded.drain(..4).collect::<Vec<_>>();
                element_len = u32::from_le_bytes(bytes.try_into().expect("is 4 bytes")) as usize;
            }

            if element_len != 0 && this.decoded.len() >= element_len {
                let decrypted_result = this
                    .cipher
                    .decrypt(&this.nonce, &this.decoded[..element_len]);

                let decrypted = match decrypted_result {
                    Ok(decrypted) => decrypted,
                    // Attempt to decrypt using a decremented nonce to support legacy archives.
                    Err(_) => {
                        this.nonce.decrement();
                        this.cipher
                            .decrypt(&this.nonce, &this.decoded[..element_len])
                            .inspect_err(|_| this.nonce.increment())?
                    }
                };

                let element = BackupElement::decode(&*decrypted);
                this.decoded.drain(..element_len);
                this.nonce.increment();
                return Poll::Ready(Some(element.map_err(ArchiveError::from)));
            }

            if amount == 0 {
                // Reader is exhausted. An empty buffer with no element in
                // flight is a clean end of stream; anything left over is a
                // partial element the archive promised but never delivered,
                // and looping again would just re-read EOF forever.
                if element_len == 0 && this.decoded.is_empty() {
                    break;
                }
                return Poll::Ready(Some(Err(std::io::Error::from(
                    std::io::ErrorKind::UnexpectedEof,
                )
                .into())));
            }
        }

        Poll::Ready(None)
    }
}

impl ArchiveImporter {
    pub async fn load(mut reader: AsyncReader, key: &[u8]) -> Result<Self, ArchiveError> {
        let mut version = [0; 2];
        reader.read_exact(&mut version).await?;
        let version = u16::from_le_bytes(version);

        let mut nonce = [0; NONCE_SIZE];
        reader.read_exact(&mut nonce).await?;

        let mut importer = Self {
            decoder: ZstdDecoder::new(reader),
            decoded: vec![],
            metadata: BackupMetadata::default(),

            #[allow(deprecated)]
            cipher: Aes256Gcm::new(GenericArray::from_slice(key)),
            #[allow(deprecated)]
            nonce: GenericArray::from(nonce),
        };

        let Some(Ok(BackupElement {
            element: Some(Element::Metadata(metadata)),
        })) = importer.next().await
        else {
            return Err(ArchiveError::MissingMetadata)?;
        };

        importer.metadata = BackupMetadata::from_metadata_save(metadata, version);
        Ok(importer)
    }

    pub fn metadata(&self) -> &BackupMetadata {
        &self.metadata
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BACKUP_VERSION;
    use async_compression::futures::write::ZstdEncoder;
    use futures_util::AsyncWriteExt;

    /// Header + a zstd payload whose length prefix promises far more bytes
    /// than the stream actually carries -- i.e. an archive truncated
    /// part-way through an element.
    async fn truncated_archive() -> Vec<u8> {
        let mut payload = 1024u32.to_le_bytes().to_vec();
        payload.extend_from_slice(&[0u8; 8]);

        let mut encoder = ZstdEncoder::new(Vec::new());
        encoder.write_all(&payload).await.unwrap();
        encoder.close().await.unwrap();

        let mut archive = BACKUP_VERSION.to_le_bytes().to_vec();
        archive.extend_from_slice(&[0u8; NONCE_SIZE]);
        archive.extend_from_slice(&encoder.into_inner());
        archive
    }

    /// A truncated archive must end the stream with an error. Before the
    /// EOF check below, `poll_next` looped on `read() == 0` forever
    /// without ever yielding, so this call never returned.
    #[xmtp_common::test]
    async fn truncated_archive_terminates_with_error() {
        let reader: AsyncReader = Box::pin(futures::io::Cursor::new(truncated_archive().await));
        let result = ArchiveImporter::load(reader, &[0u8; 32]).await;
        assert!(
            result.is_err(),
            "a truncated archive must surface an error instead of spinning"
        );
    }
}
