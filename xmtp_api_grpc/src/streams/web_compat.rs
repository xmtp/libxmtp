//! Compatibility layer for JS-Fetch POST streams & gRPC Tonic Web
//!
//! a web ['fetch' request](https://developer.mozilla.org/en-US/docs/Web/API/Fetch_API/Using_Fetch)
//! may complete succesfully, but the fetch promise does not resolve until the first bytes of the
//! body are received by the browser.
//!
//! This poses a behavior incosistency between gRPC native - HTTP/2 and gRPC-web HTTP/1.1. On
//! native, gRPC streams do not block on the first body received, while web streams do.
//! This is particularly obvious in tests, where:
//! 1. stream is started
//! 2. data is sent (for instance, group messages)
//! 3. inspect sent data
//!
//! on web, we never get past step 1.)
//!
//! This solution models web stream request as part of the stream.
//! Once the initial promise request resolves, the stream continues polling the
//! resulting response object.
//!
//! This problem is not unique to grpc-web, and must be solved for grpc-gateway streams as well
//! [code example for  grpc-gateway](https://github.com/xmtp/libxmtp/blob/87338b819730ade4c292937e3243b16e3cdee248/xmtp_api_http/src/http_stream.rs#L165)
//!
//! In context of gRPC, this should not break anything that already works with native -- grpc requests, even
//! unary requests, are all modeled as streams (a unary request is a stream with a single message),
//! and none block on receipt of the body. Ideally, we could check the header status and ensure the
//! initial response is 200 (OK), although the browser environment constraints does not allow for
//! this.

use futures::{future::FusedFuture, stream::FusedStream, Stream, StreamExt};
use pin_project_lite::pin_project;
use std::{
    future::Future,
    marker::PhantomData,
    pin::Pin,
    task::{ready, Context, Poll},
};
use tonic::{Response, Status, Streaming};

pin_project! {
    /// The establish future for the http post stream
    struct StreamEstablish<'a, F, T> {
        #[pin] inner: F,
        _marker: PhantomData<(&'a F, T)>
    }
}

impl<F, T> StreamEstablish<'_, F, T> {
    fn new(inner: F) -> Self {
        Self {
            inner,
            _marker: PhantomData,
        }
    }
}

impl<'a, F, T> Future for StreamEstablish<'a, F, T>
where
    F: Future<Output = Result<Response<Streaming<T>>, Status>>,
{
    type Output = Result<Streaming<T>, Status>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        use Poll::*;
        let this = self.as_mut().project();
        let response = ready!(this.inner.poll(cx));
        let stream = response.inspect_err(|e| {
            tracing::error!("Error during grpc-web subscription establishment {e}");
        })?;
        Ready(Ok(stream.into_inner()))
    }
}

pin_project! {
    /// The establish future for the http post stream
    #[project = ProjectStream]
    enum StreamState<'a, F, S, T> {
        NotStarted {
            #[pin] future: StreamEstablish<'a, F, T>,
        },
        Started {
            #[pin] stream: S,
        },
        Terminated
    }
}

pin_project! {
    pub struct NonBlockingWebStream<'a, F, S, T> {
        #[pin] state: StreamState<'a, F, S, T>,
    }
}

impl<F, S, T> NonBlockingWebStream<'_, F, S, T>
where
    F: Future<Output = Result<Response<Streaming<T>>, Status>>,
{
    pub(super) fn new(request: F) -> Self {
        Self {
            state: StreamState::NotStarted {
                future: StreamEstablish::new(request),
            },
        }
    }

    /// Internal API to contruct a started variant
    fn started(stream: S) -> Self {
        Self {
            state: StreamState::Started { stream },
        }
    }
}

impl<F, S, T> NonBlockingWebStream<'_, F, S, T>
where
    S: From<Streaming<T>>,
    S: Stream<Item = Result<T, Status>> + Send,
    F: Future<Output = Result<Response<Streaming<T>>, Status>>,
    F: Unpin,
    S: Unpin,
{
    /// Send the request
    pub async fn send(&mut self) -> Result<(), Status> {
        if cfg!(all(target_family = "wasm", target_os = "unknown")) {
            self.establish().await;
        }
        if cfg!(not(all(target_family = "wasm", target_os = "unknown"))) {
            let new = (&mut *self).await?;
            *self = new;
        }
        Ok(())
    }
}

/// Polls NonBlockingWeb Stream until it enters "Started" state.
/// This preserves the original behavior of the request (for native)
impl<'a, F, S, T> Future for NonBlockingWebStream<'a, F, S, T>
where
    F: Future<Output = Result<Response<Streaming<T>>, Status>>,
    S: From<Streaming<T>> + Send,
{
    type Output = Result<NonBlockingWebStream<'a, F, S, T>, Status>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        use ProjectStream::*;
        let mut this = self.as_mut().project();
        match this.state.as_mut().project() {
            NotStarted { future } => match ready!(future.poll(cx)) {
                Ok(stream) => Poll::Ready(Ok(NonBlockingWebStream::<_, S, _>::started(S::from(
                    stream,
                )))),
                Err(e) => {
                    this.state.set(StreamState::Terminated);
                    return Poll::Ready(Err(e));
                }
            },
            Started { .. } => unreachable!(),
            Terminated => unreachable!(),
        }
    }
}

impl<F, S, T> FusedFuture for NonBlockingWebStream<'_, F, S, T>
where
    F: Future<Output = Result<Response<Streaming<T>>, Status>>,
    S: From<Streaming<T>> + Send,
{
    fn is_terminated(&self) -> bool {
        matches!(
            self.state,
            StreamState::Started { .. } | StreamState::Terminated
        )
    }
}

impl<F, S, T> Stream for NonBlockingWebStream<'_, F, S, T>
where
    S: From<Streaming<T>> + Send,
    S: Stream<Item = Result<T, Status>> + Send,
    F: Future<Output = Result<Response<Streaming<T>>, Status>>,
{
    type Item = Result<T, Status>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        use ProjectStream::*;
        let mut this = self.as_mut().project();
        match this.state.as_mut().project() {
            NotStarted { future } => {
                match ready!(future.poll(cx)) {
                    Ok(stream) => {
                        this.state.set(StreamState::Started {
                            stream: S::from(stream),
                        });
                    }
                    Err(e) => {
                        this.state.set(StreamState::Terminated);
                        return Poll::Ready(Some(Err(e)));
                    }
                }
                tracing::trace!("stream ready, polling for the first time...");
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            Started { mut stream } => stream.as_mut().poll_next(cx),
            Terminated => Poll::Ready(None),
        }
    }
}

impl<F, S, T> NonBlockingWebStream<'_, F, S, T>
where
    F: Future<Output = Result<Response<Streaming<T>>, Status>>,
    S: From<Streaming<T>> + Send,
    S: Stream<Item = Result<T, Status>>,
    S: Unpin,
    F: Unpin,
{
    /// Establish the initial Stream connection
    async fn establish(&mut self) {
        // we need to poll the future once to progress the future state &
        // actually send the request in the first place.
        // since the request has not even been sent yet, this should always be pending.
        let noop_waker = futures::task::noop_waker();
        let mut cx = std::task::Context::from_waker(&noop_waker);
        let mut this = Pin::new(self);
        if this.poll_next_unpin(&mut cx).is_ready() {
            tracing::error!("Stream ready before established");
            unreachable!("Stream ready before established")
        }
    }
}

impl<F, S, T> FusedStream for NonBlockingWebStream<'_, F, S, T>
where
    F: Future<Output = Result<Response<Streaming<T>>, Status>>,
    S: Stream<Item = Result<T, Status>> + FusedStream + From<Streaming<T>> + Send,
{
    fn is_terminated(&self) -> bool {
        match &self.state {
            StreamState::Started { stream } => stream.is_terminated(),
            StreamState::Terminated => true,
            _ => false,
        }
    }
}

impl<F, S, T> std::fmt::Debug for NonBlockingWebStream<'_, F, S, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        match self.state {
            StreamState::NotStarted { .. } => write!(f, "not started"),
            StreamState::Started { .. } => write!(f, "started"),
            StreamState::Terminated => write!(f, "terminated"),
        }
    }
}

#[cfg(test)]
mod tests {
    use futures::Stream;
    use prost::bytes::Bytes;
    use tonic::codec::Codec;
    use xmtp_proto::codec::TransparentCodec;

    use super::*;

    struct TestStream;
    impl Stream for TestStream {
        type Item = Result<Response<Bytes>, Status>;

        fn poll_next(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            unreachable!()
        }
    }

    impl FusedStream for TestStream {
        fn is_terminated(&self) -> bool {
            unreachable!()
        }
    }

    impl<T> From<Streaming<T>> for TestStream {
        fn from(_: Streaming<T>) -> Self {
            unreachable!()
        }
    }

    #[xmtp_common::test]
    fn handles_err_on_establish() {
        let stream: NonBlockingWebStream<_, TestStream, _> =
            NonBlockingWebStream::new(futures::future::ready({
                // we just need something that creates a reqwest error
                // we also use now_or_never to guarantee this will trigger an error on the first poll
                Err(Status::internal("test error"))
            }));
        futures::pin_mut!(stream);

        assert!(matches!(stream.state, StreamState::NotStarted { .. }));
        let cx = futures::task::noop_waker();
        let mut cx = std::task::Context::from_waker(&cx);
        assert!(matches!(
            stream.as_mut().poll_next(&mut cx),
            Poll::Ready(Some(Err(_)))
        ));

        assert!(FusedStream::is_terminated(&stream));
        assert!(matches!(
            stream.as_mut().poll_next(&mut cx),
            Poll::Ready(None)
        ));
    }

    #[xmtp_common::test]
    fn does_not_panic_after_future_finshes() {
        let stream: NonBlockingWebStream<_, TestStream, _> =
            NonBlockingWebStream::new(futures::future::ready({
                // we just need something that creates a reqwest error
                // we also use now_or_never to guarantee this will trigger an error on the first poll
                Ok(Response::new(Streaming::new_empty(
                    TransparentCodec::default().decoder(),
                    String::new(),
                )))
            }));
        futures::pin_mut!(stream);

        assert!(matches!(stream.state, StreamState::NotStarted { .. }));
        let cx = futures::task::noop_waker();
        let mut cx = std::task::Context::from_waker(&cx);
        let result = stream.as_mut().poll(&mut cx);
        assert!(FusedFuture::is_terminated(&stream));
        let no_panic = stream.as_mut().poll(&mut cx);
        assert!(FusedFuture::is_terminated(&stream));
    }
}
