use crate::ErrorWrapper;
use crate::client::Client;
use napi::bindgen_prelude::{BigInt, Result, Uint8Array};
use napi_derive::napi;
use xmtp_id::associations::DeserializationError;
use xmtp_mls::groups::device_sync::{
  AvailableArchive, DeviceSyncError,
  archive::{
    ArchiveImporter, BackupMetadata, ENC_KEY_SIZE, exporter::ArchiveExporter, insert_importer,
  },
};
use xmtp_proto::xmtp::device_sync::{BackupElementSelection, BackupOptions};

/// Options for creating or sending an archive
#[napi(object)]
pub struct ArchiveOptions {
  pub start_ns: Option<BigInt>,
  pub end_ns: Option<BigInt>,
  pub elements: Vec<BackupElementSelectionOption>,
  pub exclude_disappearing_messages: bool,
}

impl From<ArchiveOptions> for BackupOptions {
  fn from(value: ArchiveOptions) -> Self {
    Self {
      start_ns: value.start_ns.map(|n| n.get_i64().0),
      end_ns: value.end_ns.map(|n| n.get_i64().0),
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

/// Selection of what elements to include in a backup
#[napi(string_enum)]
pub enum BackupElementSelectionOption {
  Messages,
  Consent,
}

impl From<BackupElementSelectionOption> for BackupElementSelection {
  fn from(value: BackupElementSelectionOption) -> Self {
    match value {
      BackupElementSelectionOption::Consent => Self::Consent,
      BackupElementSelectionOption::Messages => Self::Messages,
    }
  }
}

impl TryFrom<BackupElementSelection> for BackupElementSelectionOption {
  type Error = DeserializationError;
  fn try_from(value: BackupElementSelection) -> std::result::Result<Self, Self::Error> {
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

/// Metadata about a backup archive
#[napi(object)]
pub struct ArchiveMetadata {
  pub backup_version: u16,
  pub elements: Vec<BackupElementSelectionOption>,
  pub exported_at_ns: BigInt,
  pub start_ns: Option<BigInt>,
  pub end_ns: Option<BigInt>,
}

impl From<BackupMetadata> for ArchiveMetadata {
  fn from(value: BackupMetadata) -> Self {
    Self {
      backup_version: value.backup_version,
      elements: value
        .elements
        .into_iter()
        .filter_map(|selection| selection.try_into().ok())
        .collect(),
      start_ns: value.start_ns.map(BigInt::from),
      end_ns: value.end_ns.map(BigInt::from),
      exported_at_ns: BigInt::from(value.exported_at_ns),
    }
  }
}

/// An available archive in the sync group
#[napi(object)]
pub struct AvailableArchiveInfo {
  pub request_id: String,
  pub metadata: ArchiveMetadata,
  pub sent_by_installation: Uint8Array,
}

impl From<AvailableArchive> for AvailableArchiveInfo {
  fn from(value: AvailableArchive) -> Self {
    Self {
      request_id: value.request_id,
      metadata: value.metadata.into(),
      sent_by_installation: Uint8Array::from(value.sent_by_installation.as_slice()),
    }
  }
}

fn check_key(key: &Uint8Array) -> Result<Vec<u8>> {
  let key_vec: Vec<u8> = key.to_vec();
  if key_vec.len() < 32 {
    return Err(napi::Error::from_reason(format!(
      "The encryption key must be at least {} bytes long.",
      ENC_KEY_SIZE
    )));
  }
  let mut key = key_vec;
  key.truncate(ENC_KEY_SIZE);
  Ok(key)
}

#[napi]
impl Client {
  /// Manually trigger a device sync request to sync records from another active device on this account.
  #[napi]
  pub async fn send_sync_request(&self) -> Result<()> {
    self
      .inner_client()
      .device_sync_client()
      .send_sync_request()
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }

  /// Manually send a sync archive to the sync group.
  /// The pin is used for reference when importing.
  #[napi]
  pub async fn send_sync_archive(
    &self,
    options: ArchiveOptions,
    server_url: String,
    pin: String,
  ) -> Result<()> {
    self
      .inner_client()
      .device_sync_client()
      .send_sync_archive(&options.into(), &server_url, &pin)
      .await
      .map_err(ErrorWrapper::from)?;
    Ok(())
  }

  /// Manually process a sync archive that matches the pin given.
  /// If no pin is given, then it will process the last archive sent.
  #[napi]
  pub async fn process_sync_archive(&self, archive_pin: Option<String>) -> Result<()> {
    self
      .inner_client()
      .device_sync_client()
      .process_archive_with_pin(archive_pin.as_deref())
      .await
      .map_err(ErrorWrapper::from)?;
    Ok(())
  }

  /// List the archives available for import in the sync group.
  /// You may need to manually sync the sync group before calling
  /// this function to see recently uploaded archives.
  #[napi]
  pub fn list_available_archives(&self, days_cutoff: i64) -> Result<Vec<AvailableArchiveInfo>> {
    let available = self
      .inner_client()
      .device_sync_client()
      .list_available_archives(days_cutoff)
      .map_err(ErrorWrapper::from)?;

    Ok(available.into_iter().map(Into::into).collect())
  }

  /// Archive application elements to file for later restoration.
  #[napi]
  pub async fn create_archive(
    &self,
    path: String,
    opts: ArchiveOptions,
    key: Uint8Array,
  ) -> Result<()> {
    let key = check_key(&key)?;
    let db = self.inner_client().context.db();
    let options: BackupOptions = opts.into();
    ArchiveExporter::export_to_file(options, db, path, &key)
      .await
      .map_err(DeviceSyncError::Archive)
      .map_err(ErrorWrapper::from)?;
    Ok(())
  }

  /// Import a previous archive from a file.
  #[napi]
  pub async fn import_archive(&self, path: String, key: Uint8Array) -> Result<()> {
    let key = check_key(&key)?;
    let mut importer = ArchiveImporter::from_file(path, &key)
      .await
      .map_err(DeviceSyncError::Archive)
      .map_err(ErrorWrapper::from)?;

    insert_importer(&mut importer, &self.inner_client().context)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }

  /// Load the metadata for an archive to see what it contains.
  /// Reads only the metadata without loading the entire file, so this function is quick.
  #[napi]
  pub async fn archive_metadata(&self, path: String, key: Uint8Array) -> Result<ArchiveMetadata> {
    let key = check_key(&key)?;
    let importer = ArchiveImporter::from_file(path, &key)
      .await
      .map_err(DeviceSyncError::Archive)
      .map_err(ErrorWrapper::from)?;

    Ok(importer.metadata.into())
  }
}
