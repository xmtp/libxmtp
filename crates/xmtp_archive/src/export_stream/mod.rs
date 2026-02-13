use futures::{Stream, ready};
use pin_project::pin_project;
use std::{
    marker::PhantomData,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicI64, Ordering},
    },
    task::{Context, Poll},
};
use xmtp_common::{MaybeSend, MaybeSendFuture, if_native, if_wasm};
use xmtp_db::{StorageError, prelude::*};
use xmtp_proto::xmtp::device_sync::{
    BackupElement, BackupElementSelection, BackupOptions, consent_backup::ConsentSave,
    group_backup::GroupSave, message_backup::GroupMessageSave,
};

pub(crate) mod consent_save;
pub(crate) mod group_save;
pub(crate) mod message_save;

if_native! {
    type BackupInputStream = Pin<Box<dyn Stream<Item = Result<Vec<BackupElement>, StorageError>> + Send>>;
}
if_wasm! {
    type BackupInputStream = Pin<Box<dyn Stream<Item = Result<Vec<BackupElement>, StorageError>>>>;
}

type BackupRecordFuture =
    Pin<Box<dyn MaybeSendFuture<Output = Result<Vec<BackupElement>, StorageError>>>>;

#[pin_project]
/// A stream that curates a collection of streams for backup.
pub(super) struct BatchExportStream {
    pub(super) buffer: Vec<BackupElement>,
    pub(super) input_streams: Vec<BackupInputStream>,
}

impl BatchExportStream {
    #[allow(deprecated)]
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
                        opts.clone(),
                    )]
                }
                BackupElementSelection::Messages => vec![
                    // Order matters here. Don't put messages before groups.
                    BackupRecordStreamer::<GroupSave, D>::new_stream(db.clone(), opts.clone()),
                    BackupRecordStreamer::<GroupMessageSave, D>::new_stream(
                        db.clone(),
                        opts.clone(),
                    ),
                ],
                BackupElementSelection::Event => vec![],
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

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.project();

        // First, try to return buffered elements
        if let Some(element) = this.buffer.pop() {
            return Poll::Ready(Some(Ok(element)));
        }

        loop {
            let Some(last) = this.input_streams.last_mut() else {
                // No streams left, we're done.
                return Poll::Ready(None);
            };

            // Poll the last stream
            match last.as_mut().poll_next(cx) {
                Poll::Ready(Some(Ok(mut buffer))) => {
                    // Reverse to maintain pop order
                    buffer.reverse();
                    *this.buffer = buffer;

                    if let Some(element) = this.buffer.pop() {
                        return Poll::Ready(Some(Ok(element)));
                    }
                    // If buffer was empty, continue loop to check next stream
                }
                Poll::Ready(Some(Err(err))) => {
                    return Poll::Ready(Some(Err(err)));
                }
                Poll::Ready(None) => {
                    // Stream is exhausted, pop it off and continue
                    this.input_streams.pop();
                }
                Poll::Pending => {
                    return Poll::Pending;
                }
            }
        }
    }
}

#[xmtp_common::async_trait]
pub(crate) trait BackupRecordProvider: MaybeSend + Sized + 'static {
    const BATCH_SIZE: i64;
    async fn backup_records<D>(
        state: Arc<BackupProviderState<D>>,
    ) -> Result<Vec<BackupElement>, StorageError>
    where
        D: MaybeSend + DbQuery + 'static;
}

pub struct BackupProviderState<D> {
    db: Arc<D>,
    cursor: AtomicI64,
    opts: BackupOptions,
}

#[pin_project]
pub(crate) struct BackupRecordStreamer<R, D> {
    provider_state: Arc<BackupProviderState<D>>,
    #[pin]
    current_future: Option<BackupRecordFuture>,
    _phantom: PhantomData<R>,
}

impl<R, D> BackupRecordStreamer<R, D>
where
    R: BackupRecordProvider + 'static,
    D: DbQuery + 'static,
{
    pub(super) fn new_stream(db: Arc<D>, opts: BackupOptions) -> BackupInputStream {
        Box::pin(Self {
            provider_state: Arc::new(BackupProviderState {
                db,
                cursor: AtomicI64::new(0),
                opts,
            }),
            _phantom: PhantomData,
            current_future: None,
        })
    }
}

impl<R, D> Stream for BackupRecordStreamer<R, D>
where
    R: BackupRecordProvider + 'static,
    D: DbQuery + 'static,
{
    type Item = Result<Vec<BackupElement>, StorageError>;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        // Create the future if it doesn't exist
        if this.current_future.is_none() {
            let fut = R::backup_records(this.provider_state.clone());
            this.current_future.set(Some(Box::pin(fut)));
        }

        // Poll the current future - use as_pin_mut() to get the pinned reference
        let current_fut = this
            .current_future
            .as_mut()
            .as_pin_mut()
            .expect("Just set to Some");
        let batch: Result<Vec<BackupElement>, StorageError> = ready!(current_fut.poll(cx));

        // Clear the future now that it's complete
        this.current_future.set(None);

        match batch {
            Ok(elements) if elements.is_empty() => {
                // No more records, stream is done
                Poll::Ready(None)
            }
            Ok(elements) => {
                // Update cursor for next batch
                this.provider_state
                    .cursor
                    .fetch_add(R::BATCH_SIZE, Ordering::SeqCst);
                Poll::Ready(Some(Ok(elements)))
            }
            Err(e) => {
                // Return error and end stream
                Poll::Ready(Some(Err(e)))
            }
        }
    }
}
