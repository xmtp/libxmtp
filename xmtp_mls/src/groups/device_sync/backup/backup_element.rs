use std::{marker::PhantomData, sync::Arc};

use futures::Stream;
use serde::{Deserialize, Serialize};
use xmtp_proto::xmtp::device_sync::{
    consent_backup::ConsentRecordSave, group_backup::GroupSave, message_backup::GroupMessageSave,
};

use crate::storage::DbConnection;

pub(crate) mod consent_save;
pub(crate) mod group_save;
pub(crate) mod message_save;

const BATCH_SIZE: i64 = 100;

#[derive(Serialize, Deserialize)]
pub enum BackupElement {
    Group(GroupSave),
    Message(GroupMessageSave),
    Consent(ConsentRecordSave),
}

trait BackupRecordProvider {
    const BATCH_SIZE: i64;
    fn backup_records(streamer: &BackupRecordStreamer<Self>) -> Vec<BackupElement>
    where
        Self: Sized;
}

pub(super) struct BackupRecordStreamer<R> {
    offset: i64,
    conn: Arc<DbConnection>,
    _phantom: PhantomData<R>,
}

impl<R> BackupRecordStreamer<R> {
    pub(super) fn new(conn: &Arc<DbConnection>) -> Self {
        Self {
            offset: 0,
            conn: conn.clone(),
            _phantom: PhantomData,
        }
    }
}

impl<R> Stream for BackupRecordStreamer<R>
where
    R: BackupRecordProvider + Unpin,
{
    type Item = Vec<BackupElement>;
    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        use std::task::Poll;

        // Get a mutable reference to self
        let this = self.get_mut();
        let batch = R::backup_records(this);

        // If no records found, we've reached the end of the stream
        if batch.is_empty() {
            return Poll::Ready(None);
        }

        // Update offset for next batch
        this.offset += R::BATCH_SIZE;

        // Return the current batch
        Poll::Ready(Some(batch))
    }
}
