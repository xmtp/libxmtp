use super::{BackupError, BackupMetadata};
use crate::{
    configuration::MUTABLE_METADATA_EXTENSION_ID,
    groups::{
        build_group_config, build_mutable_permissions_extension,
        build_starting_group_membership_extension,
        device_sync::{DeviceSyncError, NONCE_SIZE},
        group_metadata::{DmMembers, GroupMetadata},
        group_mutable_metadata::GroupMutableMetadata,
        group_permissions::PolicySet,
        scoped_client::ScopedGroupClient,
        GroupError, MlsGroup,
    },
    storage::{
        consent_record::StoredConsentRecord, group::StoredGroup, group_message::StoredGroupMessage,
        StorageError,
    },
    Store, XmtpOpenMlsProvider,
};
use aes_gcm::{aead::Aead, aes::Aes256, Aes256Gcm, AesGcm, KeyInit};
use async_compression::futures::bufread::ZstdDecoder;
use futures_util::{AsyncBufRead, AsyncReadExt};
use openmls::{
    group::{GroupId, MlsGroup as OpenMlsGroup},
    prelude::{CredentialWithKey, Extension, Metadata, UnknownExtension},
};
use prost::Message;
use sha2::digest::{generic_array::GenericArray, typenum};
use std::pin::Pin;
use xmtp_id::associations::DeserializationError;
use xmtp_proto::xmtp::device_sync::{
    backup_element::Element, group_backup::GroupSave, BackupElement,
};

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
                return Ok(Some(
                    element.map_err(|e| StorageError::Deserialization(e.to_string()))?,
                ));
            }

            if amount == 0 && self.decoded.is_empty() {
                break;
            }
        }

        Ok(None)
    }

    pub async fn insert<Client: ScopedGroupClient>(
        &mut self,
        client: &Client,
        provider: &XmtpOpenMlsProvider,
    ) -> Result<(), DeviceSyncError> {
        while let Some(element) = self.next_element().await? {
            match insert(client, provider, element) {
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

fn insert<Client: ScopedGroupClient>(
    client: &Client,
    provider: &XmtpOpenMlsProvider,
    element: BackupElement,
) -> Result<(), DeviceSyncError> {
    let Some(element) = element.element else {
        return Ok(());
    };
    let conn = provider.conn_ref();

    match element {
        Element::Consent(consent) => {
            let consent: StoredConsentRecord = consent.try_into()?;
            consent.store(conn)?;
        }
        Element::Group(group) => {
            tracing::info!("Inserting group: {:?}", group.id);
            MlsGroup::restore_group_save(provider, client, group)?;
        }
        Element::GroupMessage(message) => {
            let message: StoredGroupMessage = message.try_into()?;
            message.store(conn)?;
        }
        _ => {}
    }

    Ok(())
}

impl<ScopedClient: ScopedGroupClient> MlsGroup<ScopedClient> {
    pub(crate) fn restore_group_save(
        provider: &XmtpOpenMlsProvider,
        client: &ScopedClient,
        group_save: GroupSave,
    ) -> Result<(), DeviceSyncError> {
        let context = client.context();
        let stored_group: StoredGroup = group_save.clone().try_into()?;

        let Some(metadata) = group_save.metdata else {
            return Err(DeserializationError::Unspecified("metadata"))?;
        };
        let dm_members = stored_group
            .dm_id
            .as_ref()
            .and_then(|dm_id| DmMembers::from_dm_id(dm_id));

        let protected_metadata = GroupMetadata::new(
            stored_group.conversation_type,
            metadata.creator_inbox_id,
            dm_members,
        );
        let protected_metadata = Metadata::new(protected_metadata.try_into()?);
        let protected_metadata = Extension::ImmutableMetadata(protected_metadata);

        let mutable_metadata: GroupMutableMetadata =
            group_save.mutable_metadata.clone().unwrap().into();
        let mutable_metadata: Vec<u8> = mutable_metadata.try_into()?;
        let mutable_metadata = Extension::Unknown(
            MUTABLE_METADATA_EXTENSION_ID,
            UnknownExtension(mutable_metadata),
        );

        let group_membership = build_starting_group_membership_extension(context.inbox_id(), 0);
        let mutable_permissions = PolicySet::new_dm();
        let mutable_permission_extension =
            build_mutable_permissions_extension(mutable_permissions)?;
        let group_config = build_group_config(
            protected_metadata,
            mutable_metadata,
            group_membership,
            mutable_permission_extension,
        )?;

        OpenMlsGroup::new_with_group_id(
            &provider,
            &context.identity.installation_keys,
            &group_config,
            GroupId::from_slice(&stored_group.id),
            CredentialWithKey {
                credential: context.identity.credential(),
                signature_key: context.identity.installation_keys.public_slice().into(),
            },
        )
        .map_err(|err| GroupError::GroupCreate(err))?;

        stored_group.store(provider.conn_ref())?;

        Ok(())
    }
}
