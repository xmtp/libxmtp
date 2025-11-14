/// a boxed version of [`Client`]
pub type BoxClient<Err> = Box<dyn BoxClientT<Err>>;

/// a type-erased version of [`Client`] in an [`Arc`](std::sync::Arc)
pub type ArcClient<Err> = Arc<dyn BoxClientT<Err>>;

use bytes::Bytes;
use futures::Stream;
use http::{request, uri::PathAndQuery};
use std::{pin::Pin, sync::Arc};
use xmtp_common::{MaybeSend, MaybeSync};

use crate::api::{ApiClientError, IsConnectedCheck};

use super::Client;

struct BoxedClient<C: ?Sized> {
    inner: C,
}

impl<C> BoxedClient<C> {
    pub fn new(client: C) -> Self {
        Self { inner: client }
    }
}

xmtp_common::if_native! {
    type BoxedStreamT<Err> = Pin<Box<dyn Stream<Item = Result<Bytes, Err>> + Send>>;
}

xmtp_common::if_wasm! {
    type BoxedStreamT<Err> = Pin<Box<dyn Stream<Item = Result<Bytes, Err>>>>;
}

pub trait BoxClientT<Err>:
    Client<Error = Err, Stream = BoxedStreamT<Err>> + IsConnectedCheck
{
}

impl<T, Err> BoxClientT<Err> for T where
    T: ?Sized + IsConnectedCheck + Client<Error = Err, Stream = BoxedStreamT<Err>>
{
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<C> Client for BoxedClient<C>
where
    C: Client,
    C::Stream: 'static,
{
    type Stream = BoxedStreamT<Self::Error>;
    type Error = C::Error;

    async fn request(
        &self,
        request: request::Builder,
        path: PathAndQuery,
        body: Bytes,
    ) -> Result<http::Response<Bytes>, ApiClientError<Self::Error>> {
        self.inner.request(request, path, body).await
    }

    async fn stream(
        &self,
        request: request::Builder,
        path: http::uri::PathAndQuery,
        body: Bytes,
    ) -> Result<http::Response<Self::Stream>, ApiClientError<Self::Error>> {
        let s = self.inner.stream(request, path, body).await?;
        let s = s.map(|s| Box::pin(s) as Pin<Box<_>>);
        Ok(s)
    }
}

pub trait ToBoxedClient {
    type Error: MaybeSend + MaybeSync;
    fn boxed(self) -> BoxClient<Self::Error>;
    fn arced(self) -> ArcClient<Self::Error>;
}

impl<C> ToBoxedClient for C
where
    C: Client + IsConnectedCheck + 'static,
    C::Stream: 'static,
{
    type Error = C::Error;
    fn boxed(self) -> BoxClient<Self::Error> {
        Box::new(BoxedClient::new(self))
    }
    fn arced(self) -> ArcClient<Self::Error> {
        Arc::new(BoxedClient::new(self))
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<T> IsConnectedCheck for BoxedClient<T>
where
    T: ?Sized + IsConnectedCheck,
{
    /// Check if a client is connected
    async fn is_connected(&self) -> bool {
        self.inner.is_connected().await
    }
}
