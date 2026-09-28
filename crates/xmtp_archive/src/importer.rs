use super::{ArchiveError, BACKUP_VERSION, BackupMetadata};
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
    element_len: Option<usize>,
    finished: bool,
    decoder: ZstdDecoder<AsyncReader>,

    cipher: AesGcm<Aes256, typenum::U12, typenum::U16>,
    #[allow(deprecated)]
    nonce: GenericArray<u8, typenum::U12>,
}

impl Stream for ArchiveImporter {
    type Item = Result<BackupElement, ArchiveError>;

    // implements: ARCH-002
    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let this = self.get_mut();

        if this.finished {
            return Poll::Ready(None);
        }

        let mut buffer = [0u8; 1024];
        loop {
            if this.element_len.is_none() && this.decoded.len() >= 4 {
                let bytes = this.decoded.drain(..4).collect::<Vec<_>>();
                let element_len =
                    u32::from_le_bytes(bytes.try_into().expect("is 4 bytes")) as usize;
                if element_len < 16 {
                    this.finished = true;
                    return Poll::Ready(Some(Err(ArchiveError::InvalidFrame(
                        "ciphertext shorter than authentication tag",
                    ))));
                }
                this.element_len = Some(element_len);
            }

            if let Some(element_len) = this.element_len
                && this.decoded.len() >= element_len
            {
                let decrypted_result = this
                    .cipher
                    .decrypt(&this.nonce, &this.decoded[..element_len]);

                let decrypted = match decrypted_result {
                    Ok(decrypted) => decrypted,
                    // Attempt to decrypt using a decremented nonce to support legacy archives.
                    Err(_) => {
                        this.nonce.decrement();
                        match this
                            .cipher
                            .decrypt(&this.nonce, &this.decoded[..element_len])
                        {
                            Ok(decrypted) => decrypted,
                            Err(error) => {
                                this.nonce.increment();
                                this.finished = true;
                                return Poll::Ready(Some(Err(error.into())));
                            }
                        }
                    }
                };

                let element = match BackupElement::decode(&*decrypted) {
                    Ok(element) => element,
                    Err(error) => {
                        this.finished = true;
                        return Poll::Ready(Some(Err(error.into())));
                    }
                };
                this.decoded.drain(..element_len);
                this.element_len = None;
                this.nonce.increment();
                return Poll::Ready(Some(Ok(element)));
            }

            let amount = match this.decoder.read(&mut buffer).poll_unpin(cx) {
                Poll::Ready(Ok(amt)) => amt,
                Poll::Ready(Err(error)) => {
                    this.finished = true;
                    return Poll::Ready(Some(Err(error.into())));
                }
                Poll::Pending => return Poll::Pending,
            };
            if amount == 0 {
                this.finished = true;
                return if this.element_len.is_none() && this.decoded.is_empty() {
                    Poll::Ready(None)
                } else {
                    Poll::Ready(Some(Err(ArchiveError::InvalidFrame("truncated frame"))))
                };
            }
            this.decoded.extend_from_slice(&buffer[..amount]);
        }
    }
}

impl ArchiveImporter {
    pub async fn load(mut reader: AsyncReader, key: &[u8]) -> Result<Self, ArchiveError> {
        let mut version = [0; 2];
        reader.read_exact(&mut version).await?;
        let version = u16::from_le_bytes(version);
        if version > BACKUP_VERSION {
            return Err(ArchiveError::UnsupportedVersion(version));
        }

        let mut nonce = [0; NONCE_SIZE];
        reader.read_exact(&mut nonce).await?;

        let mut importer = Self {
            decoder: ZstdDecoder::new(reader),
            decoded: vec![],
            element_len: None,
            finished: false,
            metadata: BackupMetadata::default(),

            #[allow(deprecated)]
            cipher: Aes256Gcm::new(GenericArray::from_slice(key)),
            #[allow(deprecated)]
            nonce: GenericArray::from(nonce),
        };

        let metadata = match importer.next().await {
            Some(Ok(BackupElement {
                element: Some(Element::Metadata(metadata)),
            })) => metadata,
            Some(Err(error)) => return Err(error),
            _ => return Err(ArchiveError::MissingMetadata),
        };

        importer.metadata = BackupMetadata::from_metadata_save(metadata, version);
        Ok(importer)
    }

    pub fn metadata(&self) -> &BackupMetadata {
        &self.metadata
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;
