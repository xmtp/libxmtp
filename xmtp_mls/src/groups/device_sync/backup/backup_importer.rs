use crate::{
    groups::device_sync::DeviceSyncError,
    storage::{
        consent_record::StoredConsentRecord, group::StoredGroup, group_message::StoredGroupMessage,
        DbConnection, ProviderTransactions, StorageError,
    },
    Store, XmtpOpenMlsProvider,
};
use async_compression::tokio::bufread::ZstdDecoder;
use prost::Message;
use std::pin::Pin;
use tokio::io::{AsyncBufRead, AsyncRead, AsyncReadExt, BufReader};
use xmtp_proto::xmtp::device_sync::{backup_element::Element, BackupElement, BackupMetadata};

use super::BackupError;

pub struct BackupImporter {
    decoded: Vec<u8>,
    decoder: ZstdDecoder<Pin<Box<dyn AsyncBufRead + Send>>>,
    pub metadata: BackupMetadata,
}

impl BackupImporter {
    pub async fn open(reader: impl AsyncRead + Send + 'static) -> Result<Self, DeviceSyncError> {
        let reader = BufReader::new(reader);
        let reader = Box::pin(reader) as Pin<Box<_>>;
        let decoder = ZstdDecoder::new(reader);

        let mut importer = Self {
            decoder,
            decoded: vec![],
            metadata: BackupMetadata::default(),
        };

        let Some(BackupElement {
            element: Some(Element::Metadata(metadata)),
        }) = importer.next_element().await?
        else {
            return Err(BackupError::MissingMetadata)?;
        };

        importer.metadata = metadata;
        Ok(importer)
    }

    async fn next_element(&mut self) -> Result<Option<BackupElement>, StorageError> {
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
                let element = BackupElement::decode(&self.decoded[..element_len]);
                self.decoded.drain(..element_len);
                return Ok(Some(
                    element.map_err(|e| StorageError::Deserialization(e.to_string()))?,
                ));
            }

            if amount == 0 && self.decoded.len() == 0 {
                break;
            }
        }

        Ok(None)
    }

    pub async fn insert(&mut self, provider: &XmtpOpenMlsProvider) -> Result<(), StorageError> {
        provider
            .transaction_async(|provider| async move {
                let conn = provider.conn_ref();
                while let Some(element) = self.next_element().await? {
                    insert(element, conn)?;
                }
                Ok(())
            })
            .await
    }

    pub fn metadata(&self) -> &BackupMetadata {
        &self.metadata
    }
}

fn insert(element: BackupElement, conn: &DbConnection) -> Result<(), StorageError> {
    let Some(element) = element.element else {
        return Ok(());
    };

    match element {
        Element::Consent(consent) => {
            let consent: StoredConsentRecord = consent.into();
            consent.store(conn)?;
        }
        Element::Group(group) => {
            let group: StoredGroup = group.into();
            group.store(conn)?;
        }
        Element::GroupMessage(message) => {
            let message: StoredGroupMessage = message.into();
            message.store(conn)?;
        }
        _ => {}
    }

    Ok(())
}
