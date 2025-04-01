use super::*;
use xmtp_proto::xmtp::device_sync::{backup_element::Element, consent_backup::ConsentSave};

impl BackupRecordProvider for ConsentSave {
    const BATCH_SIZE: i64 = 100;
    fn backup_records(streamer: &BackupRecordStreamer<Self>) -> Vec<BackupElement>
    where
        Self: Sized,
    {
        // TODO: Remove the panic
        let batch = streamer
            .provider
            .conn_ref()
            .consent_records_paged(Self::BATCH_SIZE, streamer.offset)
            .expect("failed to load from db");

        batch
            .into_iter()
            .map(|record| BackupElement {
                element: Some(Element::Consent(record.into())),
            })
            .collect()
    }
}
