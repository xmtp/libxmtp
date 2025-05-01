use super::{BackupError, BackupMetadata};
use crate::{
    groups::{
        device_sync::{DeviceSyncError, NONCE_SIZE},
        group_permissions::PolicySet,
        scoped_client::ScopedGroupClient,
        GroupError, GroupMetadataOptions, MlsGroup,
    },
    XmtpOpenMlsProvider,
};
use aes_gcm::{aead::Aead, aes::Aes256, Aes256Gcm, AesGcm, KeyInit};
use async_compression::futures::bufread::ZstdDecoder;
use futures_util::{AsyncBufRead, AsyncReadExt};
use prost::Message;
use sha2::digest::{generic_array::GenericArray, typenum};
use std::pin::Pin;
use xmtp_db::{
    consent_record::StoredConsentRecord, group::GroupMembershipState,
    group_message::StoredGroupMessage, StorageError, Store,
};
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
    pub(crate) async fn load(
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

    pub async fn run<Client>(&mut self, client: &Client) -> Result<(), DeviceSyncError>
    where
        Client: ScopedGroupClient,
    {
        let provider = client.mls_provider()?;
        while let Some(element) = self.next_element().await? {
            match insert(element, client, &provider) {
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

fn insert<Client>(
    element: BackupElement,
    client: &Client,
    provider: &XmtpOpenMlsProvider,
) -> Result<(), DeviceSyncError>
where
    Client: ScopedGroupClient,
{
    let Some(element) = element.element else {
        return Ok(());
    };

    match element {
        Element::Consent(consent) => {
            let consent: StoredConsentRecord = consent.try_into()?;
            ignore_unique_constraints(provider.conn_ref().insert_newer_consent_record(consent))?;
        }
        Element::Group(save) => {
            if let Ok(Some(_)) = provider.conn_ref().find_group(&save.id) {
                // Do not restore groups that already exist.
                return Ok(());
            }

            let attributes = save
                .mutable_metadata
                .map(|m| m.attributes)
                .unwrap_or_default();

            let result = MlsGroup::insert(
                client,
                provider,
                Some(&save.id),
                GroupMembershipState::Restored,
                PolicySet::default(),
                GroupMetadataOptions {
                    name: attributes.get("group_name").cloned(),
                    image_url_square: attributes.get("group_image_url_square").cloned(),
                    description: attributes.get("description").cloned(),
                    ..Default::default()
                },
            );

            if let Err(GroupError::Storage(storage_error)) = result {
                ignore_unique_constraints::<()>(Err(storage_error))?;
            } else {
                result?;
            }
        }
        Element::GroupMessage(message) => {
            let message: StoredGroupMessage = message.try_into()?;
            ignore_unique_constraints(message.store(provider.conn_ref()))?;
        }
        _ => {}
    }

    Ok(())
}

// If the record is already there, it's fine. Backup does not overwrite existing records.
fn ignore_unique_constraints<T>(result: Result<T, StorageError>) -> Result<(), StorageError> {
    use xmtp_db::diesel::result::{DatabaseErrorKind::UniqueViolation, Error as DieselError};
    match result {
        Err(StorageError::DieselResult(DieselError::DatabaseError(UniqueViolation, _))) => Ok(()),
        Ok(_) => Ok(()),
        Err(err) => Err(err),
    }
}
