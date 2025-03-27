use super::{BackupError, BackupMetadata};
use crate::{
    groups::device_sync::{DeviceSyncError, NONCE_SIZE},
    storage::{
        consent_record::StoredConsentRecord, group::StoredGroup, group_message::StoredGroupMessage,
        DbConnection,
    },
    Store, XmtpOpenMlsProvider,
};
use aes_gcm::{aead::Aead, aes::Aes256, Aes256Gcm, AesGcm, KeyInit};
use async_compression::futures::bufread::ZstdDecoder;
use futures_util::{AsyncBufRead, AsyncReadExt};
use prost::Message;
use sha2::digest::{generic_array::GenericArray, typenum};
use std::pin::Pin;
use xmtp_proto::xmtp::device_sync::{backup_element::Element, BackupElement};

#[cfg(not(target_arch = "wasm32"))]
mod file_import;

pub struct BackupImporter {
    pub metadata: BackupMetadata,
    decoded: Vec<u8>,
    decoder: ZstdDecoder<Pin<Box<dyn AsyncBufRead + Send>>>,

    cipher: AesGcm<Aes256, typenum::U12, typenum::U16>,
    nonce: GenericArray<u8, typenum::U12>,
}

impl BackupImporter {
    pub(super) async fn load(
        mut reader: Pin<Box<dyn AsyncBufRead + Send>>,
        key: &[u8],
    ) -> Result<Self, DeviceSyncError> {
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
            return Err(BackupError::MissingMetadata)?;
        };

        importer.metadata = BackupMetadata::from_metadata_save(metadata, version);
        Ok(importer)
    }

    async fn next_element(&mut self) -> Result<Option<BackupElement>, DeviceSyncError> {
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
                return Ok(Some(element.map_err(DeviceSyncError::from)?));
            }

            if amount == 0 && self.decoded.is_empty() {
                break;
            }
        }

        Ok(None)
    }

    pub async fn insert(&mut self, provider: &XmtpOpenMlsProvider) -> Result<(), DeviceSyncError> {
        let conn = provider.conn_ref();

        while let Some(element) = self.next_element().await? {
            match insert(element, conn) {
                Err(DeviceSyncError::Deserialization(err)) => {
                    tracing::warn!("Unable to insert record: {err:?}");
                }
                Err(err) => return Err(err)?,
                _ => {}
            }
        }

        Ok(())
    }

    pub fn metadata(&self) -> &BackupMetadata {
        &self.metadata
    }
}

fn insert(element: BackupElement, conn: &DbConnection) -> Result<(), DeviceSyncError> {
    let Some(element) = element.element else {
        return Ok(());
    };

    match element {
        Element::Consent(consent) => {
            let consent: StoredConsentRecord = consent.try_into()?;
            consent.store(conn)?;
        }
        Element::Group(group) => {
            let group: StoredGroup = group.try_into()?;
            group.store(conn)?;
        }
        Element::GroupMessage(message) => {
            let message: StoredGroupMessage = message.try_into()?;
            message.store(conn)?;
        }
        _ => {}
    }

    Ok(())
}
