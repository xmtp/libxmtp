//! Compatibility layer for JS-Fetch POST streams & gRPC Tonic Web
//!
//! a web ['fetch' request](https://developer.mozilla.org/en-US/docs/Web/API/Fetch_API/Using_Fetch)
//! may complete succesfully, but the fetch promise does not resolve until the first bytes of the
//! body are received by the browser.[issue](https://github.com/devashishdxt/tonic-web-wasm-client/issues/22).
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

use futures::{stream::FusedStream, Stream, TryFuture, TryStream};
use pin_project_lite::pin_project;
use std::{
    future::Future,
    pin::Pin,
    task::{ready, Context, Poll},
};
use tonic::Status;

use crate::streams::IntoInner;

pin_project! {
    /// The establish future for the http post stream
    struct StreamEstablish<F> {
        #[pin] inner: F,
    }
}

impl<F> StreamEstablish<F> {
    fn new(inner: F) -> Self {
        Self { inner }
    }
}

impl<F> Future for StreamEstablish<F>
where
    F: TryFuture<Error = Status>,
{
    type Output = Result<F::Ok, Status>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        use Poll::*;
        let this = self.as_mut().project();
        let response = ready!(this.inner.try_poll(cx));
        let response = response.inspect_err(|e| {
            tracing::error!("Error during grpc-web subscription establishment {e}");
        })?;
        Ready(Ok(response))
    }
}

pin_project! {
    /// The establish future for the http post stream
    #[project = ProjectStream]
    enum StreamState< F, S> {
        NotStarted {
            #[pin] future: StreamEstablish<F>,
        },
        Started {
            #[pin] stream: S,
        },
        Terminated
    }
}

pin_project! {
    pub struct NonBlockingWebStream<F, S> {
        #[pin] state: StreamState<F, S>,
    }
}

impl<F, S> NonBlockingWebStream<F, S>
where
    F: TryFuture<Error = Status>,
{
    pub fn new(request: F) -> Self {
        Self {
            state: StreamState::NotStarted {
                future: StreamEstablish::new(request),
            },
        }
    }

    /// Internal API to contruct a started variant
    pub fn started(stream: S) -> Self {
        Self {
            state: StreamState::Started { stream },
        }
    }
}

impl<F, S> Stream for NonBlockingWebStream<F, S>
where
    S: TryStream<Error = Status>,
    F: TryFuture<Error = Status>,
    F::Ok: IntoInner<Out = S>,
{
    type Item = Result<S::Ok, Status>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        use ProjectStream::*;
        let mut this = self.as_mut().project();
        match this.state.as_mut().project() {
            NotStarted { future } => {
                match ready!(future.poll(cx)) {
                    Ok(stream) => {
                        this.state.set(StreamState::Started {
                            stream: stream.into_inner(),
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
            Started { mut stream } => {
                tracing::info!("started; polling");
                stream.as_mut().try_poll_next(cx)
            }
            Terminated => Poll::Ready(None),
        }
    }
}

impl<F, S> FusedStream for NonBlockingWebStream<F, S>
where
    F: TryFuture<Error = Status>,
    S: TryStream<Error = Status> + FusedStream,
    F::Ok: IntoInner<Out = S>,
{
    fn is_terminated(&self) -> bool {
        match &self.state {
            StreamState::Started { stream } => stream.is_terminated(),
            StreamState::Terminated => true,
            _ => false,
        }
    }
}

impl<F, S> std::fmt::Debug for NonBlockingWebStream<F, S> {
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
    use tonic::{Response, Streaming};

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

    struct MockFut;
    impl Future for MockFut {
        type Output = Result<TestStream, Status>;

        fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
            unimplemented!()
        }
    }

    impl IntoInner for MockFut {
        type Out = TestStream;

        fn into_inner(self) -> Self::Out {
            todo!()
        }
    }

    #[xmtp_common::test]
    fn handles_err_on_establish() {
        let stream: NonBlockingWebStream<_, TestStream> =
            NonBlockingWebStream::new(futures::future::ready({
                // we just need something that creates a reqwest error
                // we also use now_or_never to guarantee this will trigger an error on the first poll
                Err::<MockFut, _>(Status::internal("test error"))
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
}
