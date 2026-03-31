/// a boxed version of [`Client`]
pub type BoxClient = Box<dyn BoxClientT>;

/// a type-erased version of [`Client`] in an [`Arc`](std::sync::Arc)
pub type ArcClient = Arc<dyn BoxClientT>;

use bytes::Bytes;
use http::{request, uri::PathAndQuery};
use std::sync::Arc;

use crate::api::{ApiClientError, BytesStream, IsConnectedCheck};

use super::Client;

struct BoxedClient<C: ?Sized> {
    inner: C,
}

impl<C> BoxedClient<C> {
    pub fn new(client: C) -> Self {
        Self { inner: client }
    }
}

pub trait BoxClientT: Client + IsConnectedCheck {}

impl<T> BoxClientT for T where T: ?Sized + IsConnectedCheck + Client {}

#[xmtp_common::async_trait]
impl<C> Client for BoxedClient<C>
where
    C: Client,
{
    async fn request(
        &self,
        request: request::Builder,
        path: PathAndQuery,
        body: Bytes,
    ) -> Result<http::Response<Bytes>, ApiClientError> {
        self.inner.request(request, path, body).await
    }

    async fn stream(
        &self,
        request: request::Builder,
        path: http::uri::PathAndQuery,
        body: Bytes,
    ) -> Result<http::Response<BytesStream>, ApiClientError> {
        self.inner.stream(request, path, body).await
    }
}

pub trait ToBoxedClient {
    fn boxed(self) -> BoxClient;
    fn arced(self) -> ArcClient;
}

impl<C> ToBoxedClient for C
where
    C: Client + IsConnectedCheck + 'static,
{
    fn boxed(self) -> BoxClient {
        Box::new(BoxedClient::new(self))
    }
    fn arced(self) -> ArcClient {
        Arc::new(BoxedClient::new(self))
    }
}

#[xmtp_common::async_trait]
impl<T> IsConnectedCheck for BoxedClient<T>
where
    T: ?Sized + IsConnectedCheck,
{
    /// Check if a client is connected
    async fn is_connected(&self) -> bool {
        self.inner.is_connected().await
    }
}
