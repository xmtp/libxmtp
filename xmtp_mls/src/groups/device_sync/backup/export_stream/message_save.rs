use super::*;
use diesel::prelude::*;
use xmtp_db::{
    group::ConversationType,
    group_message::{GroupMessageKind, StoredGroupMessage},
    schema::{group_messages, groups},
};
use xmtp_proto::xmtp::device_sync::{backup_element::Element, message_backup::GroupMessageSave};

impl BackupRecordProvider for GroupMessageSave {
    const BATCH_SIZE: i64 = 100;
    fn backup_records(streamer: &BackupRecordStreamer<Self>) -> Vec<BackupElement>
    where
        Self: Sized,
    {
        let mut query = group_messages::table
            .left_join(groups::table)
            .filter(groups::conversation_type.ne(ConversationType::Sync))
            .filter(group_messages::kind.eq(GroupMessageKind::Application))
            .select(group_messages::all_columns)
            .order_by(group_messages::id)
            .into_boxed();

        if let Some(start_ns) = streamer.start_ns {
            query = query.filter(group_messages::sent_at_ns.gt(start_ns));
        }
        if let Some(end_ns) = streamer.end_ns {
            query = query.filter(group_messages::sent_at_ns.le(end_ns));
        }

        query = query.limit(Self::BATCH_SIZE).offset(streamer.offset);

        let batch = streamer
            .provider
            .conn_ref()
            .raw_query_read(|conn| query.load::<StoredGroupMessage>(conn))
            .expect("Failed to load group records");

        batch
            .into_iter()
            .map(|record| BackupElement {
                element: Some(Element::GroupMessage(record.into())),
            })
            .collect()
    }
}
