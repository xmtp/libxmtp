use super::*;

use xmtp_db::client_events::ClientEvents;
use xmtp_proto::xmtp::device_sync::{
    backup_element::Element, client_event_backup::ClientEventSave,
};

impl BackupRecordProvider for ClientEventSave {
    const BATCH_SIZE: i64 = 100;
    fn backup_records<C>(
        streamer: &BackupRecordStreamer<Self, C>,
    ) -> Result<Vec<BackupElement>, StorageError>
    where
        Self: Sized,
        C: ConnectionExt,
    {
        let batch = ClientEvents::all_events_paged(
            streamer.provider.db(),
            Self::BATCH_SIZE,
            streamer.cursor,
        )?;

        let records = batch
            .into_iter()
            .map(|r| BackupElement {
                element: Some(Element::ClientEvent(ClientEventSave {
                    created_at_ns: r.created_at_ns,
                    details: serde_json::to_vec(&r.details).unwrap(),
                })),
            })
            .collect();

        Ok(records)
    }
}
