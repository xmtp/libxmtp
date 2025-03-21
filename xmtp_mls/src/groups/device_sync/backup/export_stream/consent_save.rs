use super::*;
use diesel::prelude::*;
use xmtp_db::{consent_record::StoredConsentRecord, schema::consent_records};
use xmtp_proto::xmtp::device_sync::{backup_element::Element, consent_backup::ConsentSave};

impl BackupRecordProvider for ConsentSave {
    const BATCH_SIZE: i64 = 100;
    fn backup_records(streamer: &BackupRecordStreamer<Self>) -> Vec<BackupElement>
    where
        Self: Sized,
    {
        let query = consent_records::table
            .order_by((consent_records::entity_type, consent_records::entity))
            .limit(Self::BATCH_SIZE)
            .offset(streamer.offset);

        let batch = streamer
            .provider
            .conn_ref()
            .raw_query_read(|conn| query.load::<StoredConsentRecord>(conn))
            .expect("Failed to load consent records");

        batch
            .into_iter()
            .map(|record| BackupElement {
                element: Some(Element::Consent(record.into())),
            })
            .collect()
    }
}
