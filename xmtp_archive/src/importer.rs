use crate::exporter::NONCE_SIZE;

use super::{ArchiveError, BackupMetadata};
use aes_gcm::{Aes256Gcm, AesGcm, KeyInit, aead::Aead, aes::Aes256};
use async_compression::futures::bufread::ZstdDecoder;
use futures_util::{AsyncBufRead, AsyncReadExt};
use prost::Message;
use sha2::digest::{generic_array::GenericArray, typenum};
use std::pin::Pin;
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

        let Some(BackupElement {
            element: Some(Element::Metadata(metadata)),
        }) = importer.next_element().await?
        else {
            return Err(ArchiveError::MissingMetadata)?;
        };

        importer.metadata = BackupMetadata::from_metadata_save(metadata, version);
        Ok(importer)
    }

    pub async fn next_element(&mut self) -> Result<Option<BackupElement>, ArchiveError> {
        let mut buffer = [0u8; 1024];
        let mut element_len = 0;
        loop {
            let amount = self.decoder.read(&mut buffer).await?;
            self.decoded.extend_from_slice(&buffer[..amount]);

            if element_len == 0 && self.decoded.len() >= 4 {
                let bytes = self.decoded.drain(..4).collect::<Vec<_>>();
                element_len = u32::from_le_bytes(bytes.try_into().expect("is 4 bytes")) as usize;
            }

            if element_len != 0 && self.decoded.len() >= element_len {
                let decrypted = self
                    .cipher
                    .decrypt(&self.nonce, &self.decoded[..element_len])?;
                let element = BackupElement::decode(&*decrypted);
                self.decoded.drain(..element_len);
                return Ok(Some(element.map_err(ArchiveError::from)?));
            }

            if amount == 0 && self.decoded.is_empty() {
                break;
            }
        }

        Ok(None)
    }

    pub fn metadata(&self) -> &BackupMetadata {
        &self.metadata
    }
}
