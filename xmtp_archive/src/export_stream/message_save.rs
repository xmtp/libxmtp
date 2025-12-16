use super::*;
use xmtp_db::group_message::MsgQueryArgs;
use xmtp_proto::xmtp::device_sync::{backup_element::Element, message_backup::GroupMessageSave};

#[xmtp_common::async_trait]
impl BackupRecordProvider for GroupMessageSave {
    const BATCH_SIZE: i64 = 100;
    async fn backup_records<D>(
        state: Arc<BackupProviderState<D>>,
    ) -> Result<Vec<BackupElement>, StorageError>
    where
        Self: Sized,
        D: DbQuery,
    {
        let args = MsgQueryArgs::builder()
            .sent_after_ns(state.opts.start_ns)
            .sent_before_ns(state.opts.end_ns)
            .exclude_disappearing(state.opts.exclude_disappearing_messages)
            .limit(Self::BATCH_SIZE)
            .build()
            .expect("could not build");

        let batch = state
            .db
            .group_messages_paged(&args, state.cursor.load(Ordering::SeqCst))
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
