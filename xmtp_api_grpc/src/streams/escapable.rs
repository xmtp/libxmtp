use std::{
    io::ErrorKind,
    pin::Pin,
    task::{ready, Poll},
};

use futures::{stream::FusedStream, Stream, TryStream};
use pin_project_lite::pin_project;
use std::error::Error;
use tonic::Status;

pin_project! {
    /// Wraps a tonic stream which exits once it encounters
    /// an unrecoverable HTTP Error.
    /// This wrapper does not try to differentiate between
    /// transient HTTP Errors unrecoverable HTTP errors.
    /// Once an error is encountered, the stream will yield the item
    /// with the error, and then end the stream.
    /// the stream is ended by returning Poll::Ready(None).
    ///
    /// These errors are treated as unrecoverable:
    ///   - ErrorKind::BrokenPipe
    ///     - BrokenPipe results from the HTTP/2 KeepAlive interval being exceeded
    pub struct EscapableTonicStream<S> {
        #[pin] inner: S,
        is_broken: bool
    }
}

impl<S> EscapableTonicStream<S> {
    pub fn new(inner: S) -> Self {
        Self {
            inner,
            is_broken: false,
        }
    }
}

fn maybe_extract_io_err(err: &Status) -> Option<&std::io::Error> {
    if let Some(source) = err.source() {
        //try to downcast to hyper error
        if let Some(hyper_err) = source.downcast_ref::<hyper::Error>() {
            if let Some(hyper_source) = hyper_err.source() {
                if let Some(s) = hyper_source.downcast_ref::<h2::Error>() {
                    return s.get_io();
                }
            }
        }
    }
    None
}

impl<S> Stream for EscapableTonicStream<S>
where
    S: TryStream<Error = Status>,
{
    type Item = Result<S::Ok, Status>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        // if we are broken, do not attempt to poll
        // the inner stream anymore
        if self.is_broken {
            return Poll::Ready(None);
        }
        let mut this = self.as_mut().project();
        let item = ready!(this.inner.as_mut().try_poll_next(cx));
        match item {
            Some(Err(e)) => {
                tracing::error!("error in tonic stream {}", e);
                // if the error is broken pipe, abort stream
                if let Some(io) = maybe_extract_io_err(&e) {
                    if io.kind() == ErrorKind::BrokenPipe {
                        *this.is_broken = true;
                        // register the next item (end of stream) with the executor
                        cx.waker().wake_by_ref();
                    }
                }
                Poll::Ready(Some(Err(e)))
            }
            i => Poll::Ready(i),
        }
    }
}

impl<S> FusedStream for EscapableTonicStream<S>
where
    S: TryStream<Error = Status>,
    S::Error: Into<Status>,
{
    fn is_terminated(&self) -> bool {
        self.is_broken
    }
}
