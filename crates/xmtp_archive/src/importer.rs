use super::{ArchiveError, BackupMetadata};
use crate::{BACKUP_VERSION, NONCE_SIZE, TAG_SIZE, util::GenericArrayExt};
use aes_gcm::{Aes256Gcm, AesGcm, KeyInit, aead::Aead, aes::Aes256};
use async_compression::futures::bufread::ZstdDecoder;
use futures::{FutureExt, Stream, StreamExt, ready};
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

    /// Yields elements until the stream ends cleanly or fails; after either, it yields nothing.
    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let this = self.get_mut();
        if this.finished {
            return Poll::Ready(None);
        }
        let item = ready!(this.poll_element(cx));
        this.finished = !matches!(item, Some(Ok(_)));
        Poll::Ready(item)
    }
}

impl ArchiveImporter {
    // implements: ARCH-002, ARCH-018
    fn poll_element(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Result<BackupElement, ArchiveError>>> {
        let mut buffer = [0u8; 1024];
        loop {
            if self.element_len.is_none() && self.decoded.len() >= 4 {
                let bytes = self.decoded.drain(..4).collect::<Vec<_>>();
                let element_len =
                    u32::from_le_bytes(bytes.try_into().expect("is 4 bytes")) as usize;
                if element_len < TAG_SIZE {
                    return Poll::Ready(Some(Err(ArchiveError::InvalidFrame(
                        "frame shorter than its authentication tag",
                    ))));
                }
                self.element_len = Some(element_len);
            }

            if let Some(element_len) = self.element_len
                && self.decoded.len() >= element_len
            {
                let ciphertext = &self.decoded[..element_len];
                let decrypted = match self.cipher.decrypt(&self.nonce, ciphertext) {
                    Ok(decrypted) => Ok(decrypted),
                    // Legacy archives encrypt every frame under the base nonce.
                    Err(_) => {
                        self.nonce.decrement();
                        self.cipher.decrypt(&self.nonce, ciphertext)
                    }
                };
                self.nonce.increment();
                let element = decrypted
                    .map_err(ArchiveError::from)
                    .and_then(|decrypted| Ok(BackupElement::decode(&*decrypted)?));
                self.decoded.drain(..element_len);
                self.element_len = None;
                return Poll::Ready(Some(element));
            }

            let amount = ready!(self.decoder.read(&mut buffer).poll_unpin(cx))?;
            if amount == 0 {
                return Poll::Ready(
                    (self.element_len.is_some() || !self.decoded.is_empty())
                        .then_some(Err(ArchiveError::InvalidFrame("truncated frame"))),
                );
            }
            self.decoded.extend_from_slice(&buffer[..amount]);
        }
    }

    // implements: ARCH-003
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
mod tests {
    use super::*;
    use crate::ENC_KEY_SIZE;
    use async_compression::futures::write::ZstdEncoder;
    use futures_util::{AsyncWriteExt, io::BufReader};
    use std::sync::mpsc;
    use xmtp_common::time::Duration;
    use xmtp_proto::xmtp::device_sync::{BackupMetadataSave, consent_backup::ConsentSave};

    const KEY: [u8; ENC_KEY_SIZE] = [3; ENC_KEY_SIZE];
    const NONCE: [u8; NONCE_SIZE] = [0xff; NONCE_SIZE];

    fn elements() -> [BackupElement; 2] {
        [
            BackupElement {
                element: Some(Element::Metadata(BackupMetadataSave::default())),
            },
            BackupElement {
                element: Some(Element::Consent(ConsentSave {
                    entity: "entity".into(),
                    ..Default::default()
                })),
            },
        ]
    }

    /// Length-prefixed ciphertexts under counter nonces, or under the base nonce when `legacy`.
    fn frames(legacy: bool, elements: &[BackupElement]) -> Vec<u8> {
        #[allow(deprecated)]
        let cipher = Aes256Gcm::new(GenericArray::from_slice(&KEY));
        #[allow(deprecated)]
        let mut nonce = GenericArray::from(NONCE);
        elements
            .iter()
            .flat_map(|element| {
                let ciphertext = cipher.encrypt(&nonce, &*element.encode_to_vec()).unwrap();
                if !legacy {
                    nonce.increment();
                }
                [(ciphertext.len() as u32).to_le_bytes().to_vec(), ciphertext].concat()
            })
            .collect()
    }

    /// The header for `version`, then `frames` as one complete zstd stream.
    async fn container(version: u16, frames: &[u8]) -> Vec<u8> {
        let mut encoder = ZstdEncoder::new(Vec::new());
        encoder.write_all(frames).await.unwrap();
        encoder.close().await.unwrap();
        [&version.to_le_bytes()[..], &NONCE, &encoder.into_inner()].concat()
    }

    /// Imports `archive` on its own thread, returning the elements after metadata or the first
    /// error. Panics if the import hangs or the stream yields anything after an error.
    fn restore(archive: Vec<u8>) -> Result<Vec<BackupElement>, ArchiveError> {
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .build()
                .unwrap();
            let result = runtime.block_on(async {
                let reader = Box::pin(BufReader::new(futures::io::Cursor::new(archive)));
                let mut importer = ArchiveImporter::load(reader, &KEY).await?;
                let mut restored = Vec::new();
                while let Some(element) = importer.next().await {
                    match element {
                        Ok(element) => restored.push(element),
                        Err(error) => {
                            let after = importer.next().await;
                            assert!(after.is_none(), "stream continued after {error}: {after:?}");
                            return Err(error);
                        }
                    }
                }
                Ok(restored)
            });
            let _ = sender.send(result);
        });
        receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("import hung or panicked")
    }

    /// A container this client cannot read, or one cut short anywhere, must end the import with an
    /// error — never hang, never keep yielding, never pass for a complete backup — while valid
    /// counter-nonce and legacy archives still restore.
    // verifies: ARCH-003, ARCH-018
    #[xmtp_common::test(unwrap_try = true)]
    async fn archive_invalid_container_rejected() {
        let [metadata, consent] = elements();
        let counter = frames(false, &[metadata.clone(), consent.clone()]);
        let legacy = frames(true, &[metadata.clone(), consent.clone()]);
        let metadata_end = frames(false, &[metadata]).len();

        // Both nonce forms of a version 0 archive restore every element.
        for framed in [&counter, &legacy] {
            assert_eq!(restore(container(0, framed).await)?, vec![consent.clone()]);
        }

        // A later version fails before any frame is read, naming the version.
        for version in [1, 2, u16::MAX] {
            let mut archive = container(0, &counter).await;
            archive[..2].copy_from_slice(&version.to_le_bytes());
            let error = restore(archive).unwrap_err();
            assert!(
                matches!(error, ArchiveError::UnsupportedVersion(v) if v == version),
                "version {version} was read: {error:?}"
            );
            assert!(error.to_string().contains(&version.to_string()));
        }

        // Every cut of the file — header, zstd frame header, blocks, checksum — fails.
        let archive = container(0, &counter).await;
        for cut in 0..archive.len() {
            assert!(
                restore(archive[..cut].to_vec()).is_err(),
                "archive cut at byte {cut} of {} restored",
                archive.len()
            );
        }

        // A complete zstd stream ending within a length prefix or ciphertext fails; only a frame
        // boundary after the metadata frame ends cleanly.
        for cut in 0..counter.len() {
            let restored = restore(container(0, &counter[..cut]).await);
            match cut {
                0 => assert!(matches!(restored, Err(ArchiveError::MissingMetadata))),
                _ if cut == metadata_end => assert_eq!(restored?, vec![]),
                _ => assert!(
                    matches!(restored, Err(ArchiveError::InvalidFrame(_))),
                    "frames cut at byte {cut} of {}: {restored:?}",
                    counter.len()
                ),
            }
        }

        // A full-length frame that authenticates under neither the counter nor the legacy nonce
        // fails with the authentication error and ends the stream.
        let mut forged = counter.clone();
        *forged.last_mut()? ^= 1;
        let restored = restore(container(0, &forged).await);
        assert!(
            matches!(restored, Err(ArchiveError::AesGcm(_))),
            "forged tag: {restored:?}"
        );

        // A declared ciphertext shorter than the tag fails even when the bytes are present.
        for len in 0..TAG_SIZE as u32 {
            let framed = [&counter[..metadata_end], &len.to_le_bytes(), &[0; 32]].concat();
            let restored = restore(container(0, &framed).await);
            assert!(
                matches!(restored, Err(ArchiveError::InvalidFrame(_))),
                "ciphertext length {len}: {restored:?}"
            );
        }
    }
}
