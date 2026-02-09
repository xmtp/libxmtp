#[cfg(test)]
mod tests;

use crate::{FfiError, FfiGroupSyncSummary, FfiXmtpClient};
use xmtp_id::associations::DeserializationError;
use xmtp_mls::groups::device_sync::{
    AvailableArchive, DeviceSyncError,
    archive::{
        ArchiveImporter, BACKUP_VERSION, BackupMetadata, ENC_KEY_SIZE, exporter::ArchiveExporter,
        insert_importer,
    },
};
use xmtp_proto::xmtp::device_sync::{BackupElementSelection, BackupOptions};

#[uniffi::export(async_runtime = "tokio")]
impl FfiXmtpClient {
    /// Manually trigger a device sync request to sync records from another active device on this account.
    pub async fn send_sync_request(
        &self,
        options: FfiArchiveOptions,
        server_url: String,
    ) -> Result<(), FfiError> {
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
    ) -> Result<(), FfiError> {
        self.inner_client
            .device_sync_client()
            .send_sync_archive(&options.into(), &server_url, &pin)
            .await?;
        Ok(())
    }

    /// Manually process a sync archive that matches the pin given.
    /// If no pin is given, then it will process the last archive sent.
    pub async fn process_sync_archive(&self, archive_pin: Option<String>) -> Result<(), FfiError> {
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
    ) -> Result<Vec<FfiAvailableArchive>, FfiError> {
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
    ) -> Result<FfiBackupMetadata, FfiError> {
        let db = self.inner_client.context.db();
        let options: BackupOptions = opts.into();
        let metadata = ArchiveExporter::export_to_file(options, db, path, &check_key(key)?)
            .await
            .map_err(DeviceSyncError::Archive)?;

        Ok(BackupMetadata::from_metadata_save(metadata, BACKUP_VERSION).into())
    }

    /// Import a previous archive from file.
    pub async fn import_archive(&self, path: String, key: Vec<u8>) -> Result<(), FfiError> {
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
    ) -> Result<FfiBackupMetadata, FfiError> {
        let importer = ArchiveImporter::from_file(path, &check_key(key)?)
            .await
            .map_err(DeviceSyncError::Archive)?;
        Ok(importer.metadata.into())
    }

    /// Manually sync all device sync groups.
    pub async fn sync_all_device_sync_groups(&self) -> Result<FfiGroupSyncSummary, FfiError> {
        self.inner_client.sync_welcomes().await?;
        let summary = self.inner_client.sync_all_device_sync_groups().await?;
        Ok(summary.into())
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

fn check_key(mut key: Vec<u8>) -> Result<Vec<u8>, FfiError> {
    if key.len() < 32 {
        return Err(FfiError::generic(format!(
            "The encryption key must be at least {} bytes long.",
            ENC_KEY_SIZE
        )));
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

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn test_check_key_too_short() {
        // Key shorter than 32 bytes should fail
        let short_key = vec![0u8; 31];
        let result = check_key(short_key);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(
            err.to_string(),
            "[GenericError::Generic] The encryption key must be at least 32 bytes long."
        );
    }

    #[test]
    fn test_check_key_exact_length() {
        // Key exactly 32 bytes should succeed
        let exact_key = vec![1u8; 32];
        let result = check_key(exact_key);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 32);
    }

    #[test]
    fn test_check_key_longer_gets_truncated() {
        // Key longer than 32 bytes should be truncated
        let long_key: Vec<u8> = (0..64).collect();
        let result = check_key(long_key);
        assert!(result.is_ok());
        let truncated = result.unwrap();
        assert_eq!(truncated.len(), 32);
        // Verify it's the first 32 bytes
        assert_eq!(truncated, (0..32).collect::<Vec<u8>>());
    }

    #[test]
    fn test_check_key_empty() {
        let empty_key = vec![];
        let result = check_key(empty_key);
        assert!(result.is_err());
    }

    #[test]
    fn test_ffi_backup_element_selection_to_backup_element_selection() {
        let messages = FfiBackupElementSelection::Messages;
        let converted: BackupElementSelection = messages.into();
        assert_eq!(converted, BackupElementSelection::Messages);

        let consent = FfiBackupElementSelection::Consent;
        let converted: BackupElementSelection = consent.into();
        assert_eq!(converted, BackupElementSelection::Consent);
    }

    #[test]
    fn test_backup_element_selection_to_ffi_backup_element_selection() {
        let messages = BackupElementSelection::Messages;
        let converted: FfiBackupElementSelection = messages.try_into().unwrap();
        assert!(matches!(converted, FfiBackupElementSelection::Messages));

        let consent = BackupElementSelection::Consent;
        let converted: FfiBackupElementSelection = consent.try_into().unwrap();
        assert!(matches!(converted, FfiBackupElementSelection::Consent));
    }

    #[test]
    fn test_backup_element_selection_unspecified_fails() {
        let unspecified = BackupElementSelection::Unspecified;
        let result: Result<FfiBackupElementSelection, _> = unspecified.try_into();
        assert!(result.is_err());
    }

    #[test]
    fn test_ffi_archive_options_to_backup_options() {
        let ffi_options = FfiArchiveOptions {
            start_ns: Some(1000),
            end_ns: Some(2000),
            elements: vec![
                FfiBackupElementSelection::Messages,
                FfiBackupElementSelection::Consent,
            ],
            exclude_disappearing_messages: true,
        };

        let backup_options: BackupOptions = ffi_options.into();
        assert_eq!(backup_options.start_ns, Some(1000));
        assert_eq!(backup_options.end_ns, Some(2000));
        assert_eq!(backup_options.elements.len(), 2);
        assert!(backup_options.exclude_disappearing_messages);
    }

    #[test]
    fn test_ffi_archive_options_empty_elements() {
        let ffi_options = FfiArchiveOptions {
            start_ns: None,
            end_ns: None,
            elements: vec![],
            exclude_disappearing_messages: false,
        };

        let backup_options: BackupOptions = ffi_options.into();
        assert_eq!(backup_options.start_ns, None);
        assert_eq!(backup_options.end_ns, None);
        assert!(backup_options.elements.is_empty());
        assert!(!backup_options.exclude_disappearing_messages);
    }

    #[test]
    fn test_backup_metadata_to_ffi_backup_metadata() {
        let metadata = BackupMetadata {
            backup_version: 1,
            elements: vec![
                BackupElementSelection::Messages,
                BackupElementSelection::Consent,
            ],
            exported_at_ns: 12345,
            start_ns: Some(100),
            end_ns: Some(200),
        };

        let ffi_metadata: FfiBackupMetadata = metadata.into();
        assert_eq!(ffi_metadata.backup_version, 1);
        assert_eq!(ffi_metadata.elements.len(), 2);
        assert_eq!(ffi_metadata.exported_at_ns, 12345);
        assert_eq!(ffi_metadata.start_ns, Some(100));
        assert_eq!(ffi_metadata.end_ns, Some(200));
    }

    #[test]
    fn test_backup_metadata_filters_unspecified_elements() {
        let metadata = BackupMetadata {
            backup_version: 1,
            elements: vec![
                BackupElementSelection::Messages,
                BackupElementSelection::Unspecified,
                BackupElementSelection::Consent,
            ],
            exported_at_ns: 12345,
            start_ns: None,
            end_ns: None,
        };

        let ffi_metadata: FfiBackupMetadata = metadata.into();
        // Unspecified should be filtered out
        assert_eq!(ffi_metadata.elements.len(), 2);
    }

    #[test]
    fn test_available_archive_to_ffi_available_archive() {
        let archive = AvailableArchive {
            pin: "1234".to_string(),
            metadata: BackupMetadata {
                backup_version: 1,
                elements: vec![BackupElementSelection::Messages],
                exported_at_ns: 12345,
                start_ns: None,
                end_ns: None,
            },
            sent_by_installation: vec![1, 2, 3, 4],
        };

        let ffi_archive: FfiAvailableArchive = archive.into();
        assert_eq!(ffi_archive.pin, "1234");
        assert_eq!(ffi_archive.sent_by_installation, vec![1, 2, 3, 4]);
        assert_eq!(ffi_archive.metadata.backup_version, 1);
    }
}
