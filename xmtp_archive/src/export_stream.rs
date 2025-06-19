use futures::{Stream, StreamExt};
use std::{marker::PhantomData, pin::Pin, sync::Arc, task::Poll};
use xmtp_db::{ConnectionExt, StorageError, XmtpOpenMlsProvider, prelude::*};
use xmtp_proto::xmtp::device_sync::{
    BackupElement, BackupElementSelection, BackupOptions, consent_backup::ConsentSave,
    event_backup::EventSave, group_backup::GroupSave, message_backup::GroupMessageSave,
};

pub(crate) mod consent_save;
pub(crate) mod event_save;
pub(crate) mod group_save;
pub(crate) mod message_save;

type BackupInputStream =
    Pin<Box<dyn Stream<Item = Result<Vec<BackupElement>, StorageError>> + Send>>;

/// A stream that curates a collection of streams for backup.
pub(super) struct BatchExportStream {
    pub(super) buffer: Vec<BackupElement>,
    pub(super) input_streams: Vec<BackupInputStream>,
}

impl BatchExportStream {
    pub(super) fn new<C, D>(opts: &BackupOptions, db: Arc<D>) -> Self
    where
        C: ConnectionExt + Send + Sync + Unpin + 'static,
        D: DbQuery<C> + Send + Sync + 'static,
    {
        let input_streams = opts
            .elements()
            .flat_map(|e| match e {
                BackupElementSelection::Consent => {
                    vec![BackupRecordStreamer::<ConsentSave, D, C>::new_stream(
                        db.clone(),
                        opts,
                    )]
                }
                BackupElementSelection::Messages => vec![
                    // Order matters here. Don't put messages before groups.
                    BackupRecordStreamer::<GroupSave, D, C>::new_stream(db.clone(), opts),
                    BackupRecordStreamer::<GroupMessageSave, D, C>::new_stream(db.clone(), opts),
                ],
                BackupElementSelection::Event => {
                    vec![
                        BackupRecordStreamer::<GroupSave, D, C>::new_stream(db.clone(), opts),
                        BackupRecordStreamer::<EventSave, D, C>::new_stream(db.clone(), opts),
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

impl Stream for BatchExportStream {
    type Item = Result<BackupElement, StorageError>;
    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();

        if let Some(element) = this.buffer.pop() {
            return Poll::Ready(Some(Ok(element)));
        }

        loop {
            let Some(last) = this.input_streams.last_mut() else {
                // No streams left, we're done.
                return Poll::Ready(None);
            };

            match last.poll_next_unpin(cx) {
                Poll::Ready(Some(buffer)) => {
                    this.buffer = match buffer {
                        Ok(buffer) => buffer,
                        Err(err) => {
                            return Poll::Ready(Some(Err(err)));
                        }
                    };

                    if let Some(element) = this.buffer.pop() {
                        return Poll::Ready(Some(Ok(element)));
                    }
                }
                Poll::Ready(None) => {
                    // It's ended - pop the stream off and continue
                    this.input_streams.pop();
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

pub(crate) trait BackupRecordProvider: Send {
    const BATCH_SIZE: i64;
    fn backup_records<D, C>(
        streamer: &BackupRecordStreamer<Self, D, C>,
    ) -> Result<Vec<BackupElement>, StorageError>
    where
        Self: Sized,
        C: ConnectionExt,
        D: DbQuery<C>;
}

pub(crate) struct BackupRecordStreamer<R, D, C> {
    cursor: i64,
    db: Arc<D>,
    start_ns: Option<i64>,
    end_ns: Option<i64>,
    _phantom: PhantomData<(R, C)>,
}

impl<R, D, C> BackupRecordStreamer<R, D, C>
where
    R: BackupRecordProvider + Unpin + 'static,
    C: ConnectionExt + Send + Sync + Unpin + 'static,
    D: DbQuery<C> + Send + Sync + 'static,
{
    pub(super) fn new_stream(db: Arc<D>, opts: &BackupOptions) -> BackupInputStream {
        let stream = Self {
            cursor: 0,
            db,
            start_ns: opts.start_ns,
            end_ns: opts.end_ns,
            _phantom: PhantomData,
        };

        Box::pin(stream)
    }
}

impl<R, D, C> Stream for BackupRecordStreamer<R, D, C>
where
    R: BackupRecordProvider + Unpin + Send,
    C: ConnectionExt + Unpin,
    D: DbQuery<C>,
{
    type Item = Result<Vec<BackupElement>, StorageError>;
    fn poll_next(
        self: Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        let batch = R::backup_records(this);

        if let Ok(batch) = &batch {
            if batch.is_empty() {
                return Poll::Ready(None);
            }
        }

        this.cursor += R::BATCH_SIZE;
        Poll::Ready(Some(batch))
    }
}
