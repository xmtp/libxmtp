//! Default XMTP Streams

use std::{
    future::Future, pin::Pin, task::{ready, Context, Poll}
};
use futures::FutureExt;
use tonic::{Response, Status, Streaming};

use crate::{error::GrpcError, streams::ResponseFuture};
use futures::{Stream, TryStream};
use pin_project_lite::pin_project;
use xmtp_proto::{traits::ApiClientError, ApiEndpoint};

pin_project! {
    /// A stream which maps the tonic error to ApiClientError, and attaches endpoint metadata
    pub struct XmtpTonicStream<S> {
        #[pin] inner: S,
        endpoint: ApiEndpoint,
    }
}

impl<S> XmtpTonicStream<S> {
    pub fn new(inner: S, endpoint: ApiEndpoint) -> Self {
        Self { inner, endpoint }
    }
}

impl<T: Send> XmtpTonicStream<super::NonBlocking<'_, T>> {

    pub async fn from_response(
        response: impl Future<Output = Result<Response<Streaming<T>>, Status>> + Send + Sync + Unpin + 'static,
         endpoint: ApiEndpoint
    ) -> Result<Self, Status> {
        let fut = Box::new(response) as Box<dyn Future<Output = Result<_, _>> + Send + Sync + Unpin>;
        let mut stream = super::NonBlocking::new(Pin::new(fut));
        stream.send().await?;
        Ok(Self::new(stream, endpoint))
    }
}

impl<S> Stream for XmtpTonicStream<S>
where
    S: TryStream,
    GrpcError: From<<S as TryStream>::Error>,
{
    type Item = Result<S::Ok, ApiClientError<GrpcError>>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.as_mut().project();
        if let Some(item) = ready!(this.inner.try_poll_next(cx)) {
            Poll::Ready(Some(
                item.map_err(|e| ApiClientError::new(self.endpoint, e.into())),
            ))
        } else {
            Poll::Ready(None)
        }
    }
}
