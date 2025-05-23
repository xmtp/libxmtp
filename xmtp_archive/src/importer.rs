use super::{ArchiveError, BackupMetadata};
use crate::NONCE_SIZE;
use aes_gcm::{Aes256Gcm, AesGcm, KeyInit, aead::Aead, aes::Aes256};
use async_compression::futures::bufread::ZstdDecoder;
use futures::{FutureExt, Stream, StreamExt};
use futures_util::{AsyncBufRead, AsyncReadExt};
use prost::Message;
use sha2::digest::{generic_array::GenericArray, typenum};
use std::{pin::Pin, task::Poll};
use xmtp_proto::xmtp::device_sync::{BackupElement, backup_element::Element};

#[cfg(not(target_arch = "wasm32"))]
mod file_import;

pub struct ArchiveImporter {
    pub metadata: BackupMetadata,
    decoded: Vec<u8>,
    decoder: ZstdDecoder<Pin<Box<dyn AsyncBufRead + Send>>>,

    cipher: AesGcm<Aes256, typenum::U12, typenum::U16>,
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
                let decrypted = this
                    .cipher
                    .decrypt(&this.nonce, &this.decoded[..element_len])?;
                let element = BackupElement::decode(&*decrypted);
                this.decoded.drain(..element_len);
                return Poll::Ready(Some(element.map_err(ArchiveError::from)));
            }

            if amount == 0 && this.decoded.is_empty() {
                break;
            }
        }

        Poll::Ready(None)
    }
}

impl ArchiveImporter {
    pub async fn load(
        mut reader: Pin<Box<dyn AsyncBufRead + Send>>,
        key: &[u8],
    ) -> Result<Self, ArchiveError> {
        let mut version = [0; 2];
        reader.read_exact(&mut version).await?;
        let version = u16::from_le_bytes(version);

        let mut nonce = [0; NONCE_SIZE];
        reader.read_exact(&mut nonce).await?;

        let mut importer = Self {
            decoder: ZstdDecoder::new(reader),
            decoded: vec![],
            metadata: BackupMetadata::default(),

            cipher: Aes256Gcm::new(GenericArray::from_slice(key)),
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
