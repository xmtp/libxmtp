use thiserror::Error;
use xmtp_common::time::now_ns;
use xmtp_proto::xmtp::device_sync::{BackupElementSelection, BackupMetadata};

// Increment on breaking changes
const BACKUP_VERSION: u32 = 0;

mod backup_exporter;
mod backup_importer;
mod backup_stream;

#[derive(Debug, Error)]
pub enum BackupError {
    #[error("Missing metadata")]
    MissingMetadata,
}

pub struct BackupOptions {
    start_ns: Option<i64>,
    end_ns: Option<i64>,
    elements: Vec<BackupElementSelection>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{builder::ClientBuilder, groups::GroupMetadataOptions};
    use backup_exporter::BackupExporter;
    use backup_importer::BackupImporter;
    use std::{fs::File, path::Path, sync::Arc};
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

        let mut exporter = BackupExporter::new(opts, &alix_provider);
        let path = Path::new("archive.zstd");
        let _ = std::fs::remove_file(path);
        exporter.write_to_file(&path).unwrap();

        let alix2_wallet = generate_local_wallet();
        let alix2 = ClientBuilder::new_test_client(&alix2_wallet).await;
        let alix2_provider = Arc::new(alix2.mls_provider().unwrap());

        let file = File::open(path).unwrap();
        let mut importer = BackupImporter::open(file).unwrap();
        importer.insert(&alix2_provider).unwrap();
    }
}
