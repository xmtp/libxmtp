use super::*;

use xmtp_db::{DbConnection, MlsProviderExt, events::Events};
use xmtp_proto::xmtp::device_sync::{backup_element::Element, event_backup::EventSave};

impl BackupRecordProvider for EventSave {
    const BATCH_SIZE: i64 = 100;
    fn backup_records<C>(
        provider: &XmtpOpenMlsProvider<C>,
        _start_ns: Option<i64>,
        _end_ns: Option<i64>,
        cursor: i64,
    ) -> Result<Vec<BackupElement>, StorageError>
    where
        Self: Sized,
        C: ConnectionExt,
    {
        let conn = DbConnection::new(provider.key_store().conn());
        let batch = Events::all_events_paged(&conn, Self::BATCH_SIZE, cursor)?;

        let records = batch
            .into_iter()
            .filter_map(|r| {
                Some(BackupElement {
                    element: Some(Element::Event(EventSave {
                        created_at_ns: r.created_at_ns,
                        group_id: r.group_id,
                        event: r.event,
                        details: serde_json::to_vec(&r.details).ok()?,
                        icon: r.icon,
                        level: r.level as i32,
                    })),
                })
            })
            .collect();

        Ok(records)
    }
}
