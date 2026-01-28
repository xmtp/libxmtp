#[cfg(test)]
mod tests;

use crate::{FfiXmtpClient, GenericError};
use xmtp_id::associations::DeserializationError;
use xmtp_mls::groups::device_sync::{
    AvailableArchive, DeviceSyncError,
    archive::{
        ArchiveImporter, BackupMetadata, ENC_KEY_SIZE, exporter::ArchiveExporter, insert_importer,
    },
};
use xmtp_proto::xmtp::device_sync::{BackupElementSelection, BackupOptions};

impl FfiXmtpClient {
    /// Manually trigger a device sync request to sync records from another active device on this account.
    pub async fn send_sync_request(&self) -> Result<(), GenericError> {
        self.inner_client
            .device_sync_client()
            .send_sync_request()
            .await?;
        Ok(())
    }

    /// Manually send a sync archive to the sync group.
    /// The pin will be later used as a reference when importing.
    pub async fn send_sync_archive(
        &self,
        options: FfiArchiveOptions,
        server_url: String,
        pin: String,
    ) -> Result<(), GenericError> {
        self.inner_client
            .device_sync_client()
            .send_sync_archive(&options.into(), &server_url, &pin)
            .await?;
        Ok(())
    }

    /// Manually process a sync archive that matches the pin given.
    /// If no pin is given, then it will process the last archive sent.
    pub async fn process_sync_archive(
        &self,
        archive_pin: Option<String>,
    ) -> Result<(), GenericError> {
        self.inner_client
            .device_sync_client()
            .process_archive_with_pin(archive_pin.as_deref())
            .await?;
        Ok(())
    }

    /// List the archives available for import in the sync group.
    /// You may need to manually sync the sync group before calling
    /// this function to see recently uploaded archives.
    pub fn list_available_archives(
        &self,
        days_cutoff: i64,
    ) -> Result<Vec<FfiAvailableArchive>, GenericError> {
        let available = self
            .inner_client
            .device_sync_client()
            .list_available_archives(days_cutoff)?;

        Ok(available.into_iter().map(Into::into).collect())
    }

    /// Archive application elements to file for later restoration.
    pub async fn create_archive(
        &self,
        path: String,
        opts: FfiArchiveOptions,
        key: Vec<u8>,
    ) -> Result<(), GenericError> {
        let db = self.inner_client.context.db();
        let options: BackupOptions = opts.into();
        ArchiveExporter::export_to_file(options, db, path, &check_key(key)?)
            .await
            .map_err(DeviceSyncError::Archive)?;
        Ok(())
    }

    /// Import a previous archive from file.
    pub async fn import_archive(&self, path: String, key: Vec<u8>) -> Result<(), GenericError> {
        let mut importer = ArchiveImporter::from_file(path, &check_key(key)?)
            .await
            .map_err(DeviceSyncError::Archive)?;
        insert_importer(&mut importer, &self.inner_client.context).await?;

        Ok(())
    }

    /// Load the metadata for an archive to see what it contains.
    /// Reads only the metadata without loading the entire file, so this function is quick.
    pub async fn archive_metadata(
        &self,
        path: String,
        key: Vec<u8>,
    ) -> Result<FfiBackupMetadata, GenericError> {
        let importer = ArchiveImporter::from_file(path, &check_key(key)?)
            .await
            .map_err(DeviceSyncError::Archive)?;
        Ok(importer.metadata.into())
    }

    /// Manually sync all device sync groups.
    pub async fn sync_all_device_sync_groups(&self) -> Result<(), GenericError> {
        self.inner_client.sync_all_device_sync_groups().await?;

        Ok(())
    }
}

#[derive(uniffi::Record)]
pub struct FfiArchiveOptions {
    pub start_ns: Option<i64>,
    pub end_ns: Option<i64>,
    pub elements: Vec<FfiBackupElementSelection>,
    pub exclude_disappearing_messages: bool,
}
impl From<FfiArchiveOptions> for BackupOptions {
    fn from(value: FfiArchiveOptions) -> Self {
        Self {
            start_ns: value.start_ns,
            end_ns: value.end_ns,
            elements: value
                .elements
                .into_iter()
                .map(|el| {
                    let element: BackupElementSelection = el.into();
                    element.into()
                })
                .collect(),
            exclude_disappearing_messages: value.exclude_disappearing_messages,
        }
    }
}

#[derive(uniffi::Enum)]
pub enum FfiBackupElementSelection {
    Messages,
    Consent,
}
impl From<FfiBackupElementSelection> for BackupElementSelection {
    fn from(value: FfiBackupElementSelection) -> Self {
        match value {
            FfiBackupElementSelection::Consent => Self::Consent,
            FfiBackupElementSelection::Messages => Self::Messages,
        }
    }
}

impl TryFrom<BackupElementSelection> for FfiBackupElementSelection {
    type Error = DeserializationError;
    fn try_from(value: BackupElementSelection) -> Result<Self, Self::Error> {
        let v = match value {
            BackupElementSelection::Consent => Self::Consent,
            BackupElementSelection::Messages => Self::Messages,
            _ => {
                return Err(DeserializationError::Unspecified(
                    "Backup Element Selection",
                ));
            }
        };
        Ok(v)
    }
}

fn check_key(mut key: Vec<u8>) -> Result<Vec<u8>, GenericError> {
    if key.len() < 32 {
        return Err(GenericError::Generic {
            err: format!(
                "The encryption key must be at least {} bytes long.",
                ENC_KEY_SIZE
            ),
        });
    }
    key.truncate(ENC_KEY_SIZE);
    Ok(key)
}

#[derive(uniffi::Record)]
pub struct FfiBackupMetadata {
    backup_version: u16,
    elements: Vec<FfiBackupElementSelection>,
    exported_at_ns: i64,
    start_ns: Option<i64>,
    end_ns: Option<i64>,
}
impl From<BackupMetadata> for FfiBackupMetadata {
    fn from(value: BackupMetadata) -> Self {
        Self {
            backup_version: value.backup_version,
            elements: value
                .elements
                .into_iter()
                .filter_map(|selection| selection.try_into().ok())
                .collect(),
            start_ns: value.start_ns,
            end_ns: value.end_ns,
            exported_at_ns: value.exported_at_ns,
        }
    }
}

#[derive(uniffi::Record)]
pub struct FfiAvailableArchive {
    pin: String,
    metadata: FfiBackupMetadata,
    sent_by_installation: Vec<u8>,
}
impl From<AvailableArchive> for FfiAvailableArchive {
    fn from(value: AvailableArchive) -> Self {
        Self {
            pin: value.pin,
            metadata: value.metadata.into(),
            sent_by_installation: value.sent_by_installation,
        }
    }
}
