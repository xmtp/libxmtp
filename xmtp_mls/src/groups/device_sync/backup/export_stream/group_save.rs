use super::*;
use diesel::prelude::*;
use xmtp_db::{
    group::{ConversationType, StoredGroup},
    schema::groups,
};
use xmtp_proto::xmtp::device_sync::backup_element::Element;

impl BackupRecordProvider for GroupSave {
    const BATCH_SIZE: i64 = 100;
    fn backup_records(streamer: &BackupRecordStreamer<Self>) -> Vec<BackupElement>
    where
        Self: Sized,
    {
        let mut query = groups::table
            .filter(groups::conversation_type.ne(ConversationType::Sync))
            .order_by(groups::id)
            .into_boxed();

        if let Some(start_ns) = streamer.start_ns {
            query = query.filter(groups::created_at_ns.gt(start_ns));
        }
        if let Some(end_ns) = streamer.end_ns {
            query = query.filter(groups::created_at_ns.le(end_ns));
        }

        query = query.limit(Self::BATCH_SIZE).offset(streamer.offset);

        let batch = streamer
            .provider
            .conn_ref()
            .raw_query_read(|conn| query.load::<StoredGroup>(conn))
            .expect("Failed to load group records");

        batch
            .into_iter()
            .map(|record| BackupElement {
                element: Some(Element::Group(record.into())),
            })
            .collect()
    }
}
