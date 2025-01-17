use super::DeviceSyncError;
use crate::storage::xmtp_openmls_provider::XmtpOpenMlsProvider;
use backup_exporter::BackupExporter;
use std::{path::Path, sync::Arc};
use thiserror::Error;
use xmtp_common::time::now_ns;
use xmtp_proto::xmtp::device_sync::{BackupElementSelection, BackupMetadata};

pub use backup_importer::BackupImporter;

// Increment on breaking changes
const BACKUP_VERSION: u32 = 0;

mod backup_exporter;
mod backup_importer;
mod export_stream;

#[derive(Debug, Error)]
pub enum BackupError {
    #[error("Missing metadata")]
    MissingMetadata,
}

pub struct BackupOptions {
    pub start_ns: Option<i64>,
    pub end_ns: Option<i64>,
    pub elements: Vec<BackupElementSelection>,
}

impl From<BackupOptions> for BackupMetadata {
    fn from(value: BackupOptions) -> Self {
        Self {
            backup_version: BACKUP_VERSION,
            end_ns: value.end_ns,
            start_ns: value.start_ns,
            elements: value.elements.iter().map(|&e| e as i32).collect(),
            exported_at_ns: now_ns(),
        }
    }
}

impl BackupOptions {
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn export_to_file(
        self,
        provider: XmtpOpenMlsProvider,
        path: impl AsRef<Path>,
        key: &[u8],
    ) -> Result<(), DeviceSyncError> {
        let provider = Arc::new(provider);
        let mut exporter = BackupExporter::new(self, &provider, key);
        exporter.write_to_file(path).await?;

        Ok(())
    }
}

#[cfg(test)]
#[cfg(not(target_arch = "wasm32"))]
mod tests {
    use super::*;
    use crate::{builder::ClientBuilder, groups::GroupMetadataOptions};
    use backup_exporter::BackupExporter;
    use backup_importer::BackupImporter;
    use std::{path::Path, sync::Arc};
    use xmtp_cryptography::utils::generate_local_wallet;

    #[tokio::test]
    async fn test_consent_sync() {
        let alix_wallet = generate_local_wallet();
        let alix = ClientBuilder::new_test_client(&alix_wallet).await;
        let alix_provider = Arc::new(alix.mls_provider().unwrap());

        let bo_wallet = generate_local_wallet();
        let bo = ClientBuilder::new_test_client(&bo_wallet).await;

        let alix_group = alix
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        alix_group
            .add_members_by_inbox_id(&[bo.inbox_id()])
            .await
            .unwrap();
        alix_group.send_message(b"hello there").await.unwrap();

        let opts = BackupOptions {
            start_ns: None,
            end_ns: None,
            elements: vec![BackupElementSelection::Messages],
        };

        let key = vec![7; 32];
        let mut exporter = BackupExporter::new(opts, &alix_provider, &key);
        let path = Path::new("archive.zstd");
        let _ = std::fs::remove_file(path);
        exporter.write_to_file(path).await.unwrap();

        let alix2_wallet = generate_local_wallet();
        let alix2 = ClientBuilder::new_test_client(&alix2_wallet).await;
        let alix2_provider = Arc::new(alix2.mls_provider().unwrap());

        let mut importer = BackupImporter::from_file(path, &key).await.unwrap();
        importer.insert(&alix2_provider).await.unwrap();

        // cleanup
        let _ = tokio::fs::remove_file(path).await;
    }
}
