use super::BackupOptions;
use crate::XmtpOpenMlsProvider;
use futures::Stream;
use std::{marker::PhantomData, pin::Pin, sync::Arc};
use xmtp_proto::xmtp::device_sync::BackupElement;

pub(crate) mod consent_save;
pub(crate) mod group_save;
pub(crate) mod message_save;

/// A union type that describes everything that can be backed up.
// #[derive(Serialize, Deserialize)]
// pub enum BackupElement {
// Group(GroupSave),
// Message(GroupMessageSave),
// Consent(ConsentRecordSave),
// }

/// A stream that curates a collection of streams for backup.
pub(super) struct BackupStream {
    pub(super) buffer: Vec<BackupElement>,
    pub(super) input_streams: Vec<Vec<Pin<Box<dyn Stream<Item = Vec<BackupElement>>>>>>,
}

impl Stream for BackupStream {
    type Item = BackupElement;
    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        use std::task::Poll;

        let this = self.get_mut();

        if let Some(element) = this.buffer.pop() {
            return Poll::Ready(Some(element));
        }

        loop {
            let Some(last) = this.input_streams.last_mut() else {
                // No streams left, we're done.
                return Poll::Ready(None);
            };
            if let Some(last) = last.last_mut() {
                let buffer = match last.as_mut().poll_next(cx) {
                    Poll::Ready(v) => v,
                    Poll::Pending => return Poll::Pending,
                };
                if let Some(buffer) = buffer {
                    this.buffer = buffer;
                    if let Some(element) = this.buffer.pop() {
                        return Poll::Ready(Some(element));
                    }
                }
            };

            this.input_streams.pop();
        }
    }
}

trait BackupRecordProvider {
    const BATCH_SIZE: i64;
    fn backup_records(streamer: &BackupRecordStreamer<Self>) -> Vec<BackupElement>
    where
        Self: Sized;
}

/// A generic struct to make it easier to stream backup records from the database
pub(super) struct BackupRecordStreamer<R> {
    offset: i64,
    provider: Arc<XmtpOpenMlsProvider>,
    start_ns: Option<i64>,
    end_ns: Option<i64>,
    _phantom: PhantomData<R>,
}

impl<R> BackupRecordStreamer<R> {
    pub(super) fn new(provider: &Arc<XmtpOpenMlsProvider>, opts: &BackupOptions) -> Self {
        Self {
            offset: 0,
            provider: provider.clone(),
            start_ns: opts.start_ns,
            end_ns: opts.end_ns,
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
