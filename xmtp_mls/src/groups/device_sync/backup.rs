use xmtp_common::time::now_ns;
use xmtp_proto::xmtp::device_sync::{BackupElementSelection, BackupMetadata};

// Increment on breaking changes
const BACKUP_VERSION: u32 = 0;

mod backup_exporter;
mod backup_importer;
mod backup_stream;

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
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);
    use std::sync::Arc;

    use backup_exporter::BackupExporter;
    use wasm_bindgen_test::wasm_bindgen_test;
    use xmtp_cryptography::utils::generate_local_wallet;

    use crate::{builder::ClientBuilder, groups::GroupMetadataOptions};

    use super::*;

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "multi_thread", worker_threads = 1))]
    #[cfg_attr(target_family = "wasm", ignore)]
    async fn test_consent_sync() {
        let alix_wallet = generate_local_wallet();
        let alix = ClientBuilder::new_test_client(&alix_wallet).await;
        let alix_provider = Arc::new(alix.mls_provider().unwrap());

        let bo_wallet = generate_local_wallet();
        let bo = ClientBuilder::new_test_client(&bo_wallet).await;

        // alix.();
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

        let exporter = BackupExporter::new(opts, &alix_provider);
        let tempdir = tempfile::TempDir::new().unwrap();
    }
}
