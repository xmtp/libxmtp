use super::*;
use xmtp_proto::xmtp::device_sync::{backup_element::Element, contact_backup::ContactSave};

#[xmtp_common::async_trait]
impl BackupRecordProvider for ContactSave {
    const BATCH_SIZE: i64 = 100;
    async fn backup_records<D>(
        state: Arc<BackupProviderState<D>>,
    ) -> Result<Vec<BackupElement>, StorageError>
    where
        Self: Sized,
        D: DbQuery,
    {
        let cursor = state.cursor.load(Ordering::SeqCst);
        let batch = state.db.contacts_paged(Self::BATCH_SIZE, cursor)?;

        let records = batch
            .into_iter()
            .map(|record| BackupElement {
                element: Some(Element::Contact(record.into())),
            })
            .collect();

        Ok(records)
    }
}
