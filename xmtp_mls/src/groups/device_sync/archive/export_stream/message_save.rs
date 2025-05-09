use super::*;
use xmtp_db::group_message::MsgQueryArgs;
use xmtp_proto::xmtp::device_sync::{backup_element::Element, message_backup::GroupMessageSave};

impl BackupRecordProvider for GroupMessageSave {
    const BATCH_SIZE: i64 = 100;
    fn backup_records<C>(
        streamer: &BackupRecordStreamer<Self, C>,
    ) -> Result<Vec<BackupElement>, StorageError>
    where
        Self: Sized,
        C: ConnectionExt,
    {
        let args = MsgQueryArgs::builder()
            .sent_after_ns(streamer.start_ns)
            .sent_before_ns(streamer.end_ns)
            .limit(Self::BATCH_SIZE)
            .build()
            .expect("could not build");

        let batch = streamer
            .provider
            .conn_ref()
            .group_messages_paged(&args, streamer.cursor)
            .expect("Failed to load group records");

        let records = batch
            .into_iter()
            .map(|record| BackupElement {
                element: Some(Element::GroupMessage(record.into())),
            })
            .collect();

        Ok(records)
    }
}
