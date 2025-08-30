//! Non Blocking Request
//! Fully awaits server_streaming request on native
//! polls server_streaming request once on wasm (requiring Unpin)
use crate::streams::NonBlockingWebStream;
use futures::FutureExt;
use pin_project_lite::pin_project;
use prost::bytes::Bytes;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tonic::{Response, Status};

pin_project! {
    pub struct NonBlockingStreamRequest<F> {
        #[pin] inner: F,
    }
}

impl<F> NonBlockingStreamRequest<F> {
    pub fn new(inner: F) -> Self {
        Self { inner }
    }
}

impl<F> Future for NonBlockingStreamRequest<F>
where
    F: Future,
{
    type Output = F::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.as_mut().project();
        this.inner.poll(cx)
    }
}

impl<F> NonBlockingStreamRequest<F>
where
    F: Future + Unpin,
{
    pub fn establish(&mut self) {
        // we need to poll the future once to progress the future state &
        // actually send the request in the first place.
        // since the request has not even been sent yet, this should always be pending.
        let noop_waker = futures::task::noop_waker();
        let mut cx = std::task::Context::from_waker(&noop_waker);
        {
            let mut this = Pin::new(self);
            if this.poll_unpin(&mut cx).is_ready() {
                tracing::error!("Stream ready before established");
                unreachable!("Stream ready before established")
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn send<F>(
    this: NonBlockingStreamRequest<F>,
) -> Result<
    Response<NonBlockingWebStream<NonBlockingStreamRequest<F>, tonic::Streaming<Bytes>>>,
    Status,
>
where
    F: Future<Output = Result<Response<tonic::Streaming<Bytes>>, Status>> + Send,
{
    let response = this.await?;
    Ok(response.map(NonBlockingWebStream::started))
}

#[cfg(target_arch = "wasm32")]
pub async fn send<F>(
    mut this: NonBlockingStreamRequest<F>,
) -> Result<
    Response<NonBlockingWebStream<NonBlockingStreamRequest<F>, tonic::Streaming<Bytes>>>,
    Status,
>
where
    F: Future<Output = Result<Response<tonic::Streaming<Bytes>>, Status>> + Unpin,
{
    this.establish();
    let body = NonBlockingWebStream::new(this);
    Ok(Response::new(body))
}

// we cant use From<> because of orphan rules
pub trait IntoInner {
    type Out;
    fn into_inner(self) -> Self::Out;
}

impl IntoInner for Response<tonic::Streaming<Bytes>> {
    type Out = tonic::Streaming<Bytes>;
    fn into_inner(self) -> Self::Out {
        Response::into_inner(self)
    }
}
