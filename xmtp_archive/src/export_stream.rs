use std::{marker::PhantomData, sync::Arc};
use xmtp_common::{MaybeSend, if_native, if_wasm};
use xmtp_db::{StorageError, prelude::*};
use xmtp_proto::xmtp::device_sync::{
    BackupElement, BackupElementSelection, BackupOptions, consent_backup::ConsentSave,
    event_backup::EventSave, group_backup::GroupSave, message_backup::GroupMessageSave,
};

pub(crate) mod consent_save;
pub(crate) mod event_save;
pub(crate) mod group_save;
pub(crate) mod message_save;

if_native! {
    type BackupInputStream = Box<dyn Iterator<Item = Result<Vec<BackupElement>, StorageError>> + Send>;
}
if_wasm! {
    type BackupInputStream = Box<dyn Iterator<Item = Result<Vec<BackupElement>, StorageError>>>;
}
/// A stream that curates a collection of streams for backup.
pub(super) struct BatchExportStream {
    pub(super) buffer: Vec<BackupElement>,
    pub(super) input_streams: Vec<BackupInputStream>,
}

impl BatchExportStream {
    pub(super) fn new<D>(opts: &BackupOptions, db: Arc<D>) -> Self
    where
        D: DbQuery + 'static,
    {
        let input_streams = opts
            .elements()
            .flat_map(|e| match e {
                BackupElementSelection::Consent => {
                    vec![BackupRecordStreamer::<ConsentSave, D>::new_stream(
                        db.clone(),
                        opts,
                    )]
                }
                BackupElementSelection::Messages => vec![
                    // Order matters here. Don't put messages before groups.
                    BackupRecordStreamer::<GroupSave, D>::new_stream(db.clone(), opts),
                    BackupRecordStreamer::<GroupMessageSave, D>::new_stream(db.clone(), opts),
                ],
                BackupElementSelection::Event => {
                    vec![
                        BackupRecordStreamer::<GroupSave, D>::new_stream(db.clone(), opts),
                        BackupRecordStreamer::<EventSave, D>::new_stream(db.clone(), opts),
                    ]
                }
                BackupElementSelection::Unspecified => vec![],
            })
            .rev()
            .collect();

        Self {
            input_streams,
            buffer: vec![],
        }
    }
}

impl Iterator for BatchExportStream {
    type Item = Result<BackupElement, StorageError>;
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(element) = self.buffer.pop() {
            return Some(Ok(element));
        }

        loop {
            let Some(last) = self.input_streams.last_mut() else {
                // No streams left, we're done.
                return None;
            };

            match last.next() {
                Some(buffer) => {
                    self.buffer = match buffer {
                        Ok(buffer) => buffer,
                        Err(err) => {
                            return Some(Err(err));
                        }
                    };

                    if let Some(element) = self.buffer.pop() {
                        return Some(Ok(element));
                    }
                }
                None => {
                    // It's ended - pop the stream off and continue
                    self.input_streams.pop();
                }
            }
        }
    }
}

pub(crate) trait BackupRecordProvider: MaybeSend {
    const BATCH_SIZE: i64;
    fn backup_records<D>(
        db: Arc<D>,
        start_ns: Option<i64>,
        end_ns: Option<i64>,
        cursor: i64,
    ) -> Result<Vec<BackupElement>, StorageError>
    where
        Self: Sized,
        D: DbQuery + 'static;
}

pub(crate) struct BackupRecordStreamer<R, D> {
    cursor: i64,
    db: Arc<D>,
    start_ns: Option<i64>,
    end_ns: Option<i64>,
    _phantom: PhantomData<R>,
}

impl<R, D> BackupRecordStreamer<R, D>
where
    R: BackupRecordProvider + 'static,
    D: DbQuery + 'static,
{
    pub(super) fn new_stream(db: Arc<D>, opts: &BackupOptions) -> BackupInputStream {
        Box::new(Self {
            cursor: 0,
            db,
            start_ns: opts.start_ns,
            end_ns: opts.end_ns,
            _phantom: PhantomData,
        })
    }
}

impl<R, D> Iterator for BackupRecordStreamer<R, D>
where
    R: BackupRecordProvider,
    D: DbQuery + 'static,
{
    type Item = Result<Vec<BackupElement>, StorageError>;
    fn next(&mut self) -> Option<Self::Item> {
        let batch = R::backup_records(self.db.clone(), self.start_ns, self.end_ns, self.cursor);

        if let Ok(batch) = &batch
            && batch.is_empty()
        {
            return None::<Self::Item>;
        }

        self.cursor += R::BATCH_SIZE;
        Some(batch)
    }
}
