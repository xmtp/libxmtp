use std::sync::Arc;

use futures::{
    AsyncReadExt,
    io::{BufReader, Cursor},
};
#[cfg(not(target_arch = "wasm32"))]
use xmtp_mls::worker::device_sync::archive::BACKUP_VERSION;
use xmtp_mls::{
    context::XmtpSharedContext,
    worker::device_sync::{
        ArchiveOptions as CoreArchiveOptions, BackupElementSelection as CoreElement,
        archive::{
            ArchiveImporter, BackupMetadata, ENC_KEY_SIZE, exporter::ArchiveExporter,
            insert_importer,
        },
    },
};

use crate::{Timestamp, XmtpError, client::CoreClient, conversation::on_sdk_worker};

#[derive(Clone, Debug, uniffi::Enum)]
pub enum ArchiveElement {
    Messages,
    Consent,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct ArchiveOptions {
    #[uniffi(default = None)]
    pub start: Option<Timestamp>,
    #[uniffi(default = None)]
    pub end: Option<Timestamp>,
    #[uniffi(default = None)]
    pub elements: Option<Vec<ArchiveElement>>,
    #[uniffi(default = false)]
    pub exclude_disappearing_messages: bool,
}

impl From<ArchiveOptions> for CoreArchiveOptions {
    fn from(value: ArchiveOptions) -> Self {
        Self {
            start_ns: value.start.map(|value| value.0),
            end_ns: value.end.map(|value| value.0),
            elements: value.elements.map_or_else(
                || vec![CoreElement::Messages, CoreElement::Consent],
                |elements| {
                    elements
                        .into_iter()
                        .map(|value| match value {
                            ArchiveElement::Messages => CoreElement::Messages,
                            ArchiveElement::Consent => CoreElement::Consent,
                        })
                        .collect()
                },
            ),
            exclude_disappearing_messages: value.exclude_disappearing_messages,
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct ArchiveMetadata {
    pub backup_version: u16,
    pub elements: Vec<ArchiveElement>,
    pub exported_at: Timestamp,
    pub start: Option<Timestamp>,
    pub end: Option<Timestamp>,
}

impl From<BackupMetadata> for ArchiveMetadata {
    fn from(value: BackupMetadata) -> Self {
        Self {
            backup_version: value.backup_version,
            elements: value
                .elements
                .into_iter()
                .filter_map(|value| match value {
                    CoreElement::Messages => Some(ArchiveElement::Messages),
                    CoreElement::Consent => Some(ArchiveElement::Consent),
                    _ => None,
                })
                .collect(),
            exported_at: Timestamp(value.exported_at_ns),
            start: value.start_ns.map(Timestamp),
            end: value.end_ns.map(Timestamp),
        }
    }
}

fn key(value: Vec<u8>) -> Result<Vec<u8>, XmtpError> {
    if value.len() != ENC_KEY_SIZE {
        return Err(XmtpError::invalid("archive key must be exactly 32 bytes"));
    }
    Ok(value)
}

#[derive(uniffi::Object)]
pub struct Archives {
    pub(crate) client: Arc<CoreClient>,
}

#[xmtp_macro::sdk_export]
impl Archives {
    pub async fn export_to_bytes(
        &self,
        key_bytes: Vec<u8>,
        options: Option<ArchiveOptions>,
    ) -> Result<Vec<u8>, XmtpError> {
        let key = key(key_bytes)?;
        let client = self.client.clone();
        on_sdk_worker(self.client.context.clone(), async move {
            let mut exporter = ArchiveExporter::new(
                options
                    .unwrap_or(ArchiveOptions {
                        start: None,
                        end: None,
                        elements: None,
                        exclude_disappearing_messages: false,
                    })
                    .into(),
                client.context.db(),
                &key,
            );
            let mut bytes = Vec::new();
            exporter
                .read_to_end(&mut bytes)
                .await
                .map_err(XmtpError::unknown)?;
            Ok(bytes)
        })
        .await
    }

    pub async fn import_from_bytes(
        &self,
        data: Vec<u8>,
        key_bytes: Vec<u8>,
    ) -> Result<(), XmtpError> {
        let key = key(key_bytes)?;
        let client = self.client.clone();
        on_sdk_worker(self.client.context.clone(), async move {
            let reader = Box::pin(BufReader::new(Cursor::new(data)));
            let mut importer = ArchiveImporter::load(reader, &key)
                .await
                .map_err(XmtpError::unknown)?;
            insert_importer(&mut importer, &client.context)
                .await
                .map_err(XmtpError::unknown)
        })
        .await
    }

    pub async fn metadata_from_bytes(
        &self,
        data: Vec<u8>,
        key_bytes: Vec<u8>,
    ) -> Result<ArchiveMetadata, XmtpError> {
        let key = key(key_bytes)?;
        on_sdk_worker(self.client.context.clone(), async move {
            let reader = Box::pin(BufReader::new(Cursor::new(data)));
            ArchiveImporter::load(reader, &key)
                .await
                .map(|importer| importer.metadata.into())
                .map_err(XmtpError::unknown)
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // verifies: ARCH-012
    #[xmtp_common::test(unwrap_try = true)]
    fn archive_key_requires_exact_length() {
        for length in [0, ENC_KEY_SIZE - 1, ENC_KEY_SIZE + 1] {
            assert!(matches!(
                key(vec![7; length]),
                Err(XmtpError::InvalidInput(_))
            ));
        }
        assert_eq!(key(vec![7; ENC_KEY_SIZE])?, vec![7; ENC_KEY_SIZE]);
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn archive_options_preserve_element_selection_and_time_bounds() {
        let all: CoreArchiveOptions = ArchiveOptions {
            start: None,
            end: None,
            elements: None,
            exclude_disappearing_messages: false,
        }
        .into();
        assert_eq!(all.elements.len(), 2);

        let empty: CoreArchiveOptions = ArchiveOptions {
            start: Some(Timestamp(7)),
            end: Some(Timestamp(9)),
            elements: Some(vec![]),
            exclude_disappearing_messages: true,
        }
        .into();
        assert!(empty.elements.is_empty());
        assert_eq!(empty.start_ns, Some(7));
        assert_eq!(empty.end_ns, Some(9));
        assert!(empty.exclude_disappearing_messages);

        let messages: CoreArchiveOptions = ArchiveOptions {
            start: None,
            end: None,
            elements: Some(vec![ArchiveElement::Messages]),
            exclude_disappearing_messages: false,
        }
        .into();
        assert_eq!(messages.elements.len(), 1);
        assert!(matches!(messages.elements[0], CoreElement::Messages));
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[xmtp_macro::sdk_export]
impl Archives {
    pub async fn export_to_file(
        &self,
        path: String,
        key_bytes: Vec<u8>,
        options: Option<ArchiveOptions>,
    ) -> Result<ArchiveMetadata, XmtpError> {
        let key = key(key_bytes)?;
        let client = self.client.clone();
        on_sdk_worker(self.client.context.clone(), async move {
            let options = options.unwrap_or(ArchiveOptions {
                start: None,
                end: None,
                elements: None,
                exclude_disappearing_messages: false,
            });
            let saved =
                ArchiveExporter::export_to_file(options.into(), client.context.db(), path, &key)
                    .await
                    .map_err(XmtpError::unknown)?;
            Ok(BackupMetadata::from_metadata_save(saved, BACKUP_VERSION).into())
        })
        .await
    }

    pub async fn import_from_file(
        &self,
        path: String,
        key_bytes: Vec<u8>,
    ) -> Result<(), XmtpError> {
        let key = key(key_bytes)?;
        let client = self.client.clone();
        on_sdk_worker(self.client.context.clone(), async move {
            let mut importer = ArchiveImporter::from_file(path, &key)
                .await
                .map_err(XmtpError::unknown)?;
            insert_importer(&mut importer, &client.context)
                .await
                .map_err(XmtpError::unknown)
        })
        .await
    }

    pub async fn metadata_from_file(
        &self,
        path: String,
        key_bytes: Vec<u8>,
    ) -> Result<ArchiveMetadata, XmtpError> {
        let key = key(key_bytes)?;
        on_sdk_worker(self.client.context.clone(), async move {
            ArchiveImporter::from_file(path, &key)
                .await
                .map(|importer| importer.metadata.into())
                .map_err(XmtpError::unknown)
        })
        .await
    }
}
