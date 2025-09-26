//! Default XMTP Stream

use prost::bytes::Bytes;
use std::{
    marker::PhantomData,
    pin::Pin,
    task::{ready, Context, Poll},
};

use crate::{api::ApiClientError, ApiEndpoint};
use futures::{Stream, TryStream};
use pin_project_lite::pin_project;

pin_project! {
    /// A stream which maps the tonic error to ApiClientError, and attaches endpoint metadata
    pub struct XmtpStream<S, T> {
        #[pin] inner: S,
        endpoint: ApiEndpoint,
        _marker: PhantomData<T>,
    }
}

impl<S, T> XmtpStream<S, T> {
    pub fn new(inner: S, endpoint: ApiEndpoint) -> Self {
        Self {
            inner,
            endpoint,
            _marker: PhantomData,
        }
    }
}

impl<S, T> Stream for XmtpStream<S, T>
where
    S: TryStream<Ok = Bytes>,
    T: prost::Message + Default,
    S::Error: std::error::Error + 'static,
{
    type Item = Result<T, ApiClientError<S::Error>>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.as_mut().project();
        if let Some(item) = ready!(this.inner.try_poll_next(cx)) {
            let res = item
                .map_err(|e| ApiClientError::new(self.endpoint, e))
                .and_then(|i| T::decode(i).map_err(ApiClientError::<S::Error>::DecodeError));
            Poll::Ready(Some(res))
        } else {
            Poll::Ready(None)
        }
    }
}
