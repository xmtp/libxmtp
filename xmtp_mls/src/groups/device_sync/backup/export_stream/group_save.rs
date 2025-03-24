use super::*;
use xmtp_db::group::GroupQueryArgs;
use xmtp_proto::xmtp::device_sync::backup_element::Element;

impl BackupRecordProvider for GroupSave {
    const BATCH_SIZE: i64 = 100;
    fn backup_records(streamer: &BackupRecordStreamer<Self>) -> Vec<BackupElement>
    where
        Self: Sized,
    {
        let mut args = GroupQueryArgs::default();

        if let Some(start_ns) = streamer.start_ns {
            args = args.created_after_ns(start_ns);
        }
        if let Some(end_ns) = streamer.end_ns {
            args = args.created_before_ns(end_ns);
        }
        args = args.limit(Self::BATCH_SIZE);

        let batch = streamer
            .provider
            .conn_ref()
            .find_groups_by_id_paged(args, streamer.offset)
            .expect("Failed to load group records");

        batch
            .into_iter()
            .map(|record| BackupElement {
                element: Some(Element::Group(record.into())),
            })
            .collect()
    }
}
