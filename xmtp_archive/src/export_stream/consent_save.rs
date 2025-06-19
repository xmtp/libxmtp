use super::*;

use xmtp_proto::xmtp::device_sync::{backup_element::Element, consent_backup::ConsentSave};

impl BackupRecordProvider for ConsentSave {
    const BATCH_SIZE: i64 = 100;
    fn backup_records<D, C>(
        provider: &XmtpOpenMlsProvider<C>,
        _start_ns: Option<i64>,
        _end_ns: Option<i64>,
        cursor: i64,
    ) -> Result<Vec<BackupElement>, StorageError>
    where
        Self: Sized,
        C: ConnectionExt,
        D: DbQuery<C>,
    {
        let conn = DbConnection::new(provider.key_store().conn());
        let batch = conn.consent_records_paged(Self::BATCH_SIZE, cursor)?;

        let records = batch
            .into_iter()
            .map(|record| BackupElement {
                element: Some(Element::Consent(record.into())),
            })
            .collect();

        Ok(records)
    }
}
