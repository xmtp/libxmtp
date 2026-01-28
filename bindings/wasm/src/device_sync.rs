use crate::client::{Client, GroupSyncSummary};
use bindings_wasm_macros::wasm_bindgen_numbered_enum;
use js_sys::Uint8Array;
use serde::{Deserialize, Serialize};
use tsify::Tsify;
use wasm_bindgen::{JsError, prelude::wasm_bindgen};
use xmtp_id::associations::DeserializationError;
use xmtp_mls::groups::device_sync::{
  AvailableArchive,
  archive::{
    ArchiveImporter, BackupMetadata, ENC_KEY_SIZE, exporter::ArchiveExporter, insert_importer,
  },
};
use xmtp_proto::xmtp::device_sync::{BackupElementSelection, BackupOptions};

/// Options for creating or sending an archive
#[derive(Clone, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi, large_number_types_as_bigints)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveOptions {
  #[tsify(optional)]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub start_ns: Option<i64>,
  #[tsify(optional)]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub end_ns: Option<i64>,
  pub elements: Vec<BackupElementSelectionOption>,
  pub exclude_disappearing_messages: bool,
}

impl From<ArchiveOptions> for BackupOptions {
  fn from(value: ArchiveOptions) -> Self {
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

/// Selection of what elements to include in a backup
#[wasm_bindgen_numbered_enum]
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

/// Metadata about a backup archive
#[derive(Clone, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi, large_number_types_as_bigints)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveMetadata {
  pub backup_version: u16,
  pub elements: Vec<BackupElementSelectionOption>,
  pub exported_at_ns: i64,
  #[tsify(optional)]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub start_ns: Option<i64>,
  #[tsify(optional)]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub end_ns: Option<i64>,
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
      start_ns: value.start_ns,
      end_ns: value.end_ns,
      exported_at_ns: value.exported_at_ns,
    }
  }
}

/// An available archive in the sync group
#[derive(Clone, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct AvailableArchiveInfo {
  pub pin: String,
  pub metadata: ArchiveMetadata,
  #[serde(with = "serde_bytes")]
  #[tsify(type = "Uint8Array")]
  pub sent_by_installation: Vec<u8>,
}

impl From<AvailableArchive> for AvailableArchiveInfo {
  fn from(value: AvailableArchive) -> Self {
    Self {
      pin: value.pin,
      metadata: value.metadata.into(),
      sent_by_installation: value.sent_by_installation,
    }
  }
}

fn check_key(key: &Uint8Array) -> Result<Vec<u8>, JsError> {
  let key_vec = key.to_vec();
  if key_vec.len() < 32 {
    return Err(JsError::new(&format!(
      "The encryption key must be at least {} bytes long.",
      ENC_KEY_SIZE
    )));
  }
  let mut key = key_vec;
  key.truncate(ENC_KEY_SIZE);
  Ok(key)
}

#[wasm_bindgen]
impl Client {
  /// Manually trigger a device sync request to sync records from another active device on this account.
  #[wasm_bindgen(js_name = sendSyncRequest)]
  pub async fn send_sync_request(&self) -> Result<(), JsError> {
    self
      .inner_client()
      .device_sync_client()
      .send_sync_request()
      .await
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;

    Ok(())
  }

  /// Manually send a sync archive to the sync group.
  /// The pin will be later used for reference when importing.
  #[wasm_bindgen(js_name = sendSyncArchive)]
  pub async fn send_sync_archive(
    &self,
    options: ArchiveOptions,
    #[wasm_bindgen(js_name = serverUrl)] server_url: String,
    pin: String,
  ) -> Result<(), JsError> {
    self
      .inner_client()
      .device_sync_client()
      .send_sync_archive(&options.into(), &server_url, &pin)
      .await
      .map_err(|e| JsError::new(&format!("{}", e)))?;
    Ok(())
  }

  /// Manually process a sync archive that matches the pin given.
  /// If no pin is given, then it will process the last archive sent.
  #[wasm_bindgen(js_name = processSyncArchive)]
  pub async fn process_sync_archive(&self, archive_pin: Option<String>) -> Result<(), JsError> {
    self
      .inner_client()
      .device_sync_client()
      .process_archive_with_pin(archive_pin.as_deref())
      .await
      .map_err(|e| JsError::new(&format!("{}", e)))?;
    Ok(())
  }

  /// List the archives available for import in the sync group.
  /// You may need to manually sync the sync group before calling
  /// this function to see recently uploaded archives.
  #[wasm_bindgen(js_name = listAvailableArchives)]
  pub fn list_available_archives(
    &self,
    #[wasm_bindgen(js_name = daysCutoff)] days_cutoff: i64,
  ) -> Result<Vec<AvailableArchiveInfo>, JsError> {
    let available = self
      .inner_client()
      .device_sync_client()
      .list_available_archives(days_cutoff)
      .map_err(|e| JsError::new(&format!("{}", e)))?;

    Ok(available.into_iter().map(Into::into).collect())
  }

  /// Export archive data to bytes for later restoration.
  #[wasm_bindgen(js_name = createArchive)]
  pub async fn create_archive(
    &self,
    opts: ArchiveOptions,
    key: Uint8Array,
  ) -> Result<Uint8Array, JsError> {
    use futures::AsyncReadExt;

    let key = check_key(&key)?;
    let db = self.inner_client().context.db();
    let options: BackupOptions = opts.into();
    let mut exporter = ArchiveExporter::new(options, db, &key);

    let mut buffer = Vec::new();
    exporter
      .read_to_end(&mut buffer)
      .await
      .map_err(|e| JsError::new(&format!("Failed to export archive: {}", e)))?;

    Ok(Uint8Array::from(buffer.as_slice()))
  }

  /// Import an archive from bytes.
  #[wasm_bindgen(js_name = importArchive)]
  pub async fn import_archive(&self, data: Uint8Array, key: Uint8Array) -> Result<(), JsError> {
    use futures::io::{BufReader, Cursor};

    let key = check_key(&key)?;
    let data = data.to_vec();

    let reader = Box::pin(BufReader::new(Cursor::new(data)));
    let mut importer = ArchiveImporter::load(reader, &key)
      .await
      .map_err(|e| JsError::new(&format!("Failed to load archive: {}", e)))?;

    insert_importer(&mut importer, &self.inner_client().context)
      .await
      .map_err(|e| JsError::new(&format!("Failed to import archive: {}", e)))?;

    Ok(())
  }

  /// Load the metadata for an archive to see what it contains.
  #[wasm_bindgen(js_name = archiveMetadata)]
  pub async fn archive_metadata(
    &self,
    data: Uint8Array,
    key: Uint8Array,
  ) -> Result<ArchiveMetadata, JsError> {
    use futures::io::{BufReader, Cursor};

    let key = check_key(&key)?;
    let data = data.to_vec();

    let reader = Box::pin(BufReader::new(Cursor::new(data)));
    let importer = ArchiveImporter::load(reader, &key)
      .await
      .map_err(|e| JsError::new(&format!("Failed to load archive: {}", e)))?;

    Ok(importer.metadata.into())
  }

  /// Manually sync all device sync groups.
  #[wasm_bindgen(js_name = syncAllDeviceSyncGroups)]
  pub async fn sync_all_device_sync_groups(&self) -> Result<GroupSyncSummary, JsError> {
    let summary = self
      .inner_client()
      .sync_all_device_sync_groups()
      .await
      .map_err(|e| JsError::new(&format!("{}", e)))?;

    Ok(summary.into())
  }
}
