use super::*;
use xmtp_proto::xmtp::device_sync::{backup_element::Element, consent_backup::ConsentSave};

#[xmtp_common::async_trait]
impl BackupRecordProvider for ConsentSave {
    const BATCH_SIZE: i64 = 100;
    async fn backup_records<D>(
        state: Arc<BackupProviderState<D>>,
    ) -> Result<Vec<BackupElement>, StorageError>
    where
        Self: Sized,
        D: DbQuery,
    {
        let cursor = state.cursor.load(Ordering::SeqCst);
        let batch = state.db.consent_records_paged(Self::BATCH_SIZE, cursor)?;

        let records = batch
            .into_iter()
            .map(|record| BackupElement {
                element: Some(Element::Consent(record.into())),
            })
            .collect();

        Ok(records)
    }
}
