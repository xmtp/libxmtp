//! Default XMTP Stream

use prost::bytes::Bytes;
use std::{
    marker::PhantomData,
    pin::Pin,
    task::{ready, Context, Poll},
};

use crate::{
    api::{ApiClientError, Client},
    ApiEndpoint,
};
use futures::{Stream, TryStream};
use pin_project_lite::pin_project;

pin_project! {
    /// A stream which maps the tonic error to ApiClientError, and attaches endpoint metadata
    pub struct XmtpStream<S, T> {
        #[pin] inner: S,
        endpoint: ApiEndpoint,
        _marker: PhantomData<T>,
    }

    impl<S, T> PinnedDrop for XmtpStream<S, T> {
        fn drop(_this: Pin<&mut Self>) {
            tracing::info!("dropped tonic stream");
        }
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

/// create a stream from the body of a request
/// makes the request and starts the stream
/// used for v3 to create a stream with the generic grpc client
///TODO:v3: can be removed when v3 is removed
pub async fn stream_from_body<B: prost::Name, C: Client, T>(
    body: B,
    client: C,
    endpoint: ApiEndpoint,
) -> Result<XmtpStream<<C as Client>::Stream, T>, <C as Client>::Error>
where
    C::Error: From<http::uri::InvalidUri>,
    C::Error: From<ApiClientError<<C as Client>::Error>>,
{
    let pnq = crate::path_and_query::<B>();
    let request = http::Request::builder();
    let path = http::uri::PathAndQuery::try_from(pnq.as_ref())?;
    let s = client
        .stream(request, path, body.encode_to_vec().into())
        .await?;
    Ok(XmtpStream::new(s.into_body(), endpoint))
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
                .and_then(|i| T::decode(i).map_err(|e| ApiClientError::<S::Error>::DecodeError(e)));
            Poll::Ready(Some(res))
        } else {
            Poll::Ready(None)
        }
    }
}
