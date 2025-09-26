//! Default XMTP Streams

use prost::bytes::Bytes;
use std::{
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll, ready},
};

use crate::error::GrpcError;
use futures::{Stream, TryStream};
use pin_project_lite::pin_project;
use xmtp_proto::{
    ApiEndpoint,
    api::{ApiClientError, Client},
};

pin_project! {
    /// A stream which maps the tonic error to ApiClientError, and attaches endpoint metadata
    pub struct XmtpTonicStream<S, T> {
        #[pin] inner: S,
        endpoint: ApiEndpoint,
        _marker: PhantomData<T>,
    }
}

impl<S, T> XmtpTonicStream<S, T> {
    pub fn new(inner: S, endpoint: ApiEndpoint) -> Self {
        Self {
            inner,
            endpoint,
            _marker: PhantomData,
        }
    }
}

impl<T> XmtpTonicStream<crate::GrpcStream, T> {
    /// create a stream from the body of a request
    /// makes the request and starts the stream
    pub async fn from_body<B: prost::Name>(
        body: B,
        client: crate::GrpcClient,
        endpoint: ApiEndpoint,
    ) -> Result<Self, ApiClientError<GrpcError>> {
        let pnq = xmtp_proto::path_and_query::<B>();
        let request = http::Request::builder();
        let path = http::uri::PathAndQuery::try_from(pnq.as_ref())?;
        let s = client
            .stream(request, path, body.encode_to_vec().into())
            .await?;
        Ok(Self::new(s.into_body(), endpoint))
    }
}

impl<S, T> Stream for XmtpTonicStream<S, T>
where
    S: TryStream<Ok = Bytes, Error = GrpcError>,
    GrpcError: From<<S as TryStream>::Error>,
    T: prost::Message + Default,
{
    type Item = Result<T, ApiClientError<GrpcError>>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.as_mut().project();
        if let Some(item) = ready!(this.inner.try_poll_next(cx)) {
            let res = item
                .map_err(|e| ApiClientError::new(self.endpoint.clone(), e))
                .and_then(|i| T::decode(i).map_err(GrpcError::from).map_err(Into::into));
            Poll::Ready(Some(res))
        } else {
            Poll::Ready(None)
        }
    }
}
