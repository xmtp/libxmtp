use std::{
    io::ErrorKind,
    marker::PhantomData,
    pin::Pin,
    task::{ready, Poll},
};

use futures::{stream::FusedStream, Stream};
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
    pub struct EscapableTonicStream<S, T> {
        #[pin] inner: S,
        _marker: PhantomData<T>,
        is_broken: bool
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

impl<S, T> Stream for EscapableTonicStream<S, T>
where
    S: Stream<Item = Result<T, Status>> + Send,
{
    type Item = Result<T, Status>;

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
        let item = ready!(this.inner.as_mut().poll_next(cx));
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

impl<S, T> FusedStream for EscapableTonicStream<S, T>
where
    S: Stream<Item = Result<T, Status>> + Send,
{
    fn is_terminated(&self) -> bool {
        self.is_broken
    }
}

impl<T> From<tonic::codec::Streaming<T>> for EscapableTonicStream<tonic::codec::Streaming<T>, T> {
    fn from(value: tonic::codec::Streaming<T>) -> Self {
        EscapableTonicStream {
            inner: value,
            is_broken: false,
            _marker: PhantomData,
        }
    }
}
