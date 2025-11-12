use super::*;
use xmtp_proto::xmtp::device_sync::{backup_element::Element, consent_backup::ConsentSave};

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl BackupRecordProvider for ConsentSave {
    const BATCH_SIZE: i64 = 100;
    async fn backup_records<D>(
        db: Arc<D>,
        _start_ns: Option<i64>,
        _end_ns: Option<i64>,
        cursor: i64,
    ) -> Result<Vec<BackupElement>, StorageError>
    where
        Self: Sized,
        D: DbQuery,
    {
        let batch = db.consent_records_paged(Self::BATCH_SIZE, cursor)?;

        let records = batch
            .into_iter()
            .map(|record| BackupElement {
                element: Some(Element::Consent(record.into())),
            })
            .collect();

        Ok(records)
    }
}
