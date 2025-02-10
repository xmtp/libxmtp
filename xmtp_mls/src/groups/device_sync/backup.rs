use super::DeviceSyncError;
use crate::storage::xmtp_openmls_provider::XmtpOpenMlsProvider;
use backup_exporter::BackupExporter;
use std::{path::Path, sync::Arc};
use thiserror::Error;
use xmtp_common::time::now_ns;
use xmtp_proto::xmtp::device_sync::{BackupElementSelection, BackupMetadataSave};

pub use backup_importer::BackupImporter;

// Increment on breaking changes
const BACKUP_VERSION: u16 = 0;

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

#[derive(Default)]
pub struct BackupMetadata {
    pub backup_version: u16,
    pub elements: Vec<BackupElementSelection>,
    pub exported_at_ns: i64,
    pub start_ns: Option<i64>,
    pub end_ns: Option<i64>,
}

impl BackupMetadata {
    fn from_metadata_save(save: BackupMetadataSave, backup_version: u16) -> Self {
        Self {
            elements: save.elements().collect(),
            end_ns: save.end_ns,
            start_ns: save.start_ns,
            exported_at_ns: save.exported_at_ns,
            backup_version,
        }
    }
}

impl From<BackupOptions> for BackupMetadataSave {
    fn from(value: BackupOptions) -> Self {
        Self {
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
mod tests {
    use super::*;
    use crate::{
        builder::ClientBuilder,
        groups::GroupMetadataOptions,
        storage::{
            consent_record::StoredConsentRecord,
            group::StoredGroup,
            group_message::StoredGroupMessage,
            schema::{consent_records, group_messages, groups},
        },
    };
    use backup_exporter::BackupExporter;
    use backup_importer::BackupImporter;
    use diesel::RunQueryDsl;
    use futures::io::Cursor;
    use std::{path::Path, sync::Arc};
    use xmtp_cryptography::utils::generate_local_wallet;

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(
        not(target_arch = "wasm32"),
        tokio::test(flavor = "multi_thread", worker_threads = 1)
    )]
    async fn test_buffer_export_import() {
        use futures::io::BufReader;
        use futures_util::AsyncReadExt;

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
            elements: vec![
                BackupElementSelection::Messages,
                BackupElementSelection::Consent,
            ],
        };

        let key = vec![7; 32];

        let file = {
            let mut file = Vec::new();
            let mut exporter = BackupExporter::new(opts, &alix_provider, &key);
            exporter.read_to_end(&mut file).await.unwrap();
            file
        };

        let alix2_wallet = generate_local_wallet();
        let alix2 = ClientBuilder::new_test_client(&alix2_wallet).await;
        let alix2_provider = Arc::new(alix2.mls_provider().unwrap());

        // No messages
        let messages: Vec<StoredGroupMessage> = alix2_provider
            .conn_ref()
            .raw_query_read(|conn| group_messages::table.load(conn))
            .unwrap();
        assert_eq!(messages.len(), 0);

        let reader = BufReader::new(Cursor::new(file));
        let reader = Box::pin(reader);
        let mut importer = BackupImporter::load(reader, &key).await.unwrap();
        importer.insert(&alix2, &alix2_provider).await.unwrap();

        // One message.
        let messages: Vec<StoredGroupMessage> = alix2_provider
            .conn_ref()
            .raw_query_read(|conn| group_messages::table.load(conn))
            .unwrap();
        assert_eq!(messages.len(), 1);
    }

    #[tokio::test]
    #[cfg(not(target_arch = "wasm32"))]
    async fn test_file_backup() {
        use crate::utils::HISTORY_SYNC_URL;

        let alix_wallet = generate_local_wallet();
        let alix =
            ClientBuilder::new_test_client_with_history(&alix_wallet, HISTORY_SYNC_URL).await;
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

        let mut consent_records: Vec<StoredConsentRecord> = alix_provider
            .conn_ref()
            .raw_query_read(|conn| consent_records::table.load(conn))
            .unwrap();
        assert_eq!(consent_records.len(), 1);
        let old_consent_record = consent_records.pop().unwrap();

        let mut groups: Vec<StoredGroup> = alix_provider
            .conn_ref()
            .raw_query_read(|conn| groups::table.load(conn))
            .unwrap();
        assert_eq!(groups.len(), 2);
        let old_group = groups.pop().unwrap();

        let old_messages: Vec<StoredGroupMessage> = alix_provider
            .conn_ref()
            .raw_query_read(|conn| group_messages::table.load(conn))
            .unwrap();
        assert_eq!(old_messages.len(), 6);

        let opts = BackupOptions {
            start_ns: None,
            end_ns: None,
            elements: vec![
                BackupElementSelection::Messages,
                BackupElementSelection::Consent,
            ],
        };

        let key = vec![7; 32];
        let mut exporter = BackupExporter::new(opts, &alix_provider, &key);
        let path = Path::new("archive.xmtp");
        let _ = std::fs::remove_file(path);
        exporter.write_to_file(path).await.unwrap();

        let alix2_wallet = generate_local_wallet();
        let alix2 = ClientBuilder::new_test_client(&alix2_wallet).await;
        let alix2_provider = Arc::new(alix2.mls_provider().unwrap());

        // No consent before
        let consent_records: Vec<StoredConsentRecord> = alix2_provider
            .conn_ref()
            .raw_query_read(|conn| consent_records::table.load(conn))
            .unwrap();
        assert_eq!(consent_records.len(), 0);

        let mut importer = BackupImporter::from_file(path, &key).await.unwrap();
        importer.insert(&alix2, &alix2_provider).await.unwrap();

        // Consent is there after the import
        let consent_records: Vec<StoredConsentRecord> = alix2_provider
            .conn_ref()
            .raw_query_read(|conn| consent_records::table.load(conn))
            .unwrap();
        assert_eq!(consent_records.len(), 1);
        // It's the same consent record.
        assert_eq!(consent_records[0], old_consent_record);

        let groups: Vec<StoredGroup> = alix2_provider
            .conn_ref()
            .raw_query_read(|conn| groups::table.load(conn))
            .unwrap();
        assert_eq!(groups.len(), 1);
        // It's the same group
        assert_eq!(groups[0].id, old_group.id);
        tracing::info!("Groups: {:?}", groups);

        let messages: Vec<StoredGroupMessage> = alix2_provider
            .conn_ref()
            .raw_query_read(|conn| group_messages::table.load(conn))
            .unwrap();
        // Only the application messages should sync
        assert_eq!(messages.len(), 1);
        for msg in messages {
            let old_msg = old_messages.iter().find(|m| msg.id == m.id).unwrap();
            assert_eq!(old_msg.authority_id, msg.authority_id);
            assert_eq!(old_msg.decrypted_message_bytes, msg.decrypted_message_bytes);
            assert_eq!(old_msg.sent_at_ns, msg.sent_at_ns);
            assert_eq!(old_msg.sender_installation_id, msg.sender_installation_id);
            assert_eq!(old_msg.sender_inbox_id, msg.sender_inbox_id);
            assert_eq!(old_msg.group_id, msg.group_id);
        }

        alix2
            .sync_all_welcomes_and_groups(&alix2_provider, None)
            .await
            .unwrap();

        // cleanup
        let _ = tokio::fs::remove_file(path).await;
    }
}
