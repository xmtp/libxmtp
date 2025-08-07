//! A future which polls the establishment of an HTTP/1.1 POST Stream
//! Once the request returns, immediately transforms the response into a Stream of Bytes.
//! If an error occurs during the request, the future fails.

use crate::HttpClientError;
use futures::{
    Future,
};
use pin_project_lite::pin_project;
use reqwest::Response;
use std::{
    marker::PhantomData,
    pin::Pin,
    task::{ready, Context, Poll},
};
use xmtp_common::StreamWrapper;

pin_project! {
    /// The establish future for the http post stream
    pub(super) struct HttpStreamEstablish<'a, F> {
        #[pin] inner: F,
        _marker: PhantomData<&'a F>
    }
}

impl<F> HttpStreamEstablish<'_, F> {
    pub(super) fn new(inner: F) -> Self {
        Self {
            inner,
            _marker: PhantomData,
        }
    }
}

impl<'a, F> Future for HttpStreamEstablish<'a, F>
where
    F: Future<Output = Result<Response, reqwest::Error>>,
{
    type Output = Result<StreamWrapper<'a, Result<bytes::Bytes, reqwest::Error>>, HttpClientError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        use Poll::*;
        let this = self.as_mut().project();
        let response = ready!(this.inner.poll(cx));
        let stream = response.inspect_err(|e| {
            tracing::error!("Error during http subscription with grpc http gateway {e}");
        })?;
        Ready(Ok(StreamWrapper::new(stream.bytes_stream())))
    }
}
