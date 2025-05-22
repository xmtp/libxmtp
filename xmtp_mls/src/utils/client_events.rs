use std::sync::Arc;

use crate::groups::device_sync::DeviceSyncError;
use xmtp_archive::exporter::ArchiveExporter;
use xmtp_db::XmtpOpenMlsProvider;
use xmtp_proto::xmtp::device_sync::{BackupElementSelection, BackupOptions};

pub async fn upload_debug_package(
    provider: &Arc<XmtpOpenMlsProvider>,
    device_sync_server_url: impl AsRef<str>,
) -> Result<String, DeviceSyncError> {
    let provider = provider.clone();
    let device_sync_server_url = device_sync_server_url.as_ref();

    let options = BackupOptions {
        elements: vec![BackupElementSelection::ClientEvent as i32],
        ..Default::default()
    };

    // Generate a random encryption key
    let key = xmtp_common::rand_vec::<32>();

    // Build the exporter
    let exporter = ArchiveExporter::new(options, provider.clone(), &key);

    let url = format!("{device_sync_server_url}/upload");
    let response = exporter.post_to_url(&url).await?;

    Ok(format!("{response}:{}", hex::encode(key)))
}
