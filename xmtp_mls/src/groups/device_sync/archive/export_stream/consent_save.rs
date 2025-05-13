use super::*;

use xmtp_proto::xmtp::device_sync::{backup_element::Element, consent_backup::ConsentSave};

impl BackupRecordProvider for ConsentSave {
    const BATCH_SIZE: i64 = 100;
    fn backup_records<C>(
        streamer: &BackupRecordStreamer<Self, C>,
    ) -> Result<Vec<BackupElement>, StorageError>
    where
        Self: Sized,
        C: ConnectionExt,
    {
        let batch = streamer
            .provider
            .db()
            .consent_records_paged(Self::BATCH_SIZE, streamer.cursor)?;

        let records = batch
            .into_iter()
            .map(|record| BackupElement {
                element: Some(Element::Consent(record.into())),
            })
            .collect();

        Ok(records)
    }
}
