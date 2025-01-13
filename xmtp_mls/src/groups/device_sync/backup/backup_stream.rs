use super::BackupOptions;
use crate::XmtpOpenMlsProvider;
use std::{marker::PhantomData, sync::Arc};
use xmtp_proto::xmtp::device_sync::{
    consent_backup::ConsentSave, group_backup::GroupSave, message_backup::GroupMessageSave,
    BackupElement, BackupElementSelection,
};

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

trait ExportStream {
    fn next(&mut self) -> Option<Vec<BackupElement>>;
}
type BackupInputStream = Box<dyn ExportStream>;

/// A stream that curates a collection of streams for backup.
pub(super) struct BackupStream {
    pub(super) buffer: Vec<BackupElement>,
    pub(super) input_streams: Vec<BackupInputStream>,
}

impl BackupStream {
    pub(super) fn new(opts: &BackupOptions, provider: &Arc<XmtpOpenMlsProvider>) -> Self {
        let input_streams = opts
            .elements
            .iter()
            .flat_map(|&e| match e {
                BackupElementSelection::Consent => {
                    vec![BackupRecordStreamer::<ConsentSave>::new(provider, opts)]
                }
                BackupElementSelection::Messages => vec![
                    BackupRecordStreamer::<GroupSave>::new(provider, opts),
                    BackupRecordStreamer::<GroupMessageSave>::new(provider, opts),
                ],
            })
            .collect();

        Self {
            input_streams,
            buffer: vec![],
        }
    }
}

impl BackupStream {
    pub(super) fn next(&mut self) -> Option<BackupElement> {
        if let Some(element) = self.buffer.pop() {
            return Some(element);
        }

        loop {
            let Some(last) = self.input_streams.last_mut() else {
                // No streams left, we're done.
                return None;
            };

            if let Some(buffer) = last.next() {
                self.buffer = buffer;
                if let Some(element) = self.buffer.pop() {
                    return Some(element);
                }
            }

            self.input_streams.pop();
        }
    }
}

pub(crate) trait BackupRecordProvider {
    const BATCH_SIZE: i64;
    fn backup_records(streamer: &BackupRecordStreamer<Self>) -> Vec<BackupElement>
    where
        Self: Sized;
}

/// A generic struct to make it easier to stream backup records from the database
pub(crate) struct BackupRecordStreamer<R> {
    offset: i64,
    provider: Arc<XmtpOpenMlsProvider>,
    start_ns: Option<i64>,
    end_ns: Option<i64>,
    _phantom: PhantomData<R>,
}

impl<R> BackupRecordStreamer<R>
where
    R: BackupRecordProvider + 'static,
{
    pub(super) fn new(
        provider: &Arc<XmtpOpenMlsProvider>,
        opts: &BackupOptions,
    ) -> BackupInputStream {
        let stream = Self {
            offset: 0,
            provider: provider.clone(),
            start_ns: opts.start_ns,
            end_ns: opts.end_ns,
            _phantom: PhantomData,
        };

        Box::new(stream)
    }
}

impl<R> ExportStream for BackupRecordStreamer<R>
where
    R: BackupRecordProvider,
{
    fn next(&mut self) -> Option<Vec<BackupElement>> {
        let batch = R::backup_records(self);

        // If no records found, we've reached the end of the stream
        if batch.is_empty() {
            return None;
        }

        // Update offset for next batch
        self.offset += R::BATCH_SIZE;

        // Return the current batch
        Some(batch)
    }
}
