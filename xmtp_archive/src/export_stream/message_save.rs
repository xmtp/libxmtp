use super::*;
use xmtp_db::MlsProviderExt;
use xmtp_db::{DbConnection, group_message::MsgQueryArgs};
use xmtp_proto::xmtp::device_sync::{backup_element::Element, message_backup::GroupMessageSave};

impl BackupRecordProvider for GroupMessageSave {
    const BATCH_SIZE: i64 = 100;
    fn backup_records<C>(
        provider: &XmtpOpenMlsProvider<C>,
        start_ns: Option<i64>,
        end_ns: Option<i64>,
        cursor: i64,
    ) -> Result<Vec<BackupElement>, StorageError>
    where
        Self: Sized,
        C: ConnectionExt,
    {
        let args = MsgQueryArgs::builder()
            .sent_after_ns(start_ns)
            .sent_before_ns(end_ns)
            .limit(Self::BATCH_SIZE)
            .build()
            .expect("could not build");

        let conn = DbConnection::new(provider.key_store().conn());
        let batch = conn
            .group_messages_paged(&args, cursor)
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
