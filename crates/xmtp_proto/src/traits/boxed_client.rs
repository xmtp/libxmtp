/// a boxed version of [`Client`]
pub type BoxClient = Box<dyn BoxClientT>;

/// An owned transport shared through an [`Arc`].
///
/// The named type keeps the trait object's lifetime out of generic async
/// return types. This lets those futures retain their native `Send` bound.
/// Clones share the same transport allocation and connection state.
#[derive(Clone)]
pub struct ArcClient(Arc<dyn BoxClientT>);

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

impl std::ops::Deref for ArcClient {
    type Target = Arc<dyn BoxClientT>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<Arc<dyn BoxClientT>> for ArcClient {
    fn from(client: Arc<dyn BoxClientT>) -> Self {
        Self(client)
    }
}

#[xmtp_common::async_trait]
impl Client for ArcClient {
    fn host(&self) -> &str {
        self.0.as_ref().host()
    }

    async fn request(
        &self,
        request: request::Builder,
        path: PathAndQuery,
        body: Bytes,
    ) -> Result<http::Response<Bytes>, ApiClientError> {
        self.0.as_ref().request(request, path, body).await
    }

    async fn stream(
        &self,
        request: request::Builder,
        path: PathAndQuery,
        body: Bytes,
    ) -> Result<http::Response<BytesStream>, ApiClientError> {
        self.0.as_ref().stream(request, path, body).await
    }

    async fn bidi_stream(
        &self,
        request: request::Builder,
        path: PathAndQuery,
        body: xmtp_common::BoxDynStream<'static, Bytes>,
    ) -> Result<http::Response<BytesStream>, ApiClientError> {
        self.0.as_ref().bidi_stream(request, path, body).await
    }

    fn fake_stream(&self) -> http::Response<BytesStream> {
        self.0.as_ref().fake_stream()
    }
}

#[xmtp_common::async_trait]
impl IsConnectedCheck for ArcClient {
    async fn is_connected(&self) -> bool {
        self.0.as_ref().is_connected().await
    }
}

#[xmtp_common::async_trait]
impl<C> Client for BoxedClient<C>
where
    C: Client,
{
    fn host(&self) -> &str {
        self.inner.host()
    }

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

    async fn bidi_stream(
        &self,
        request: request::Builder,
        path: http::uri::PathAndQuery,
        body: xmtp_common::BoxDynStream<'static, Bytes>,
    ) -> Result<http::Response<BytesStream>, ApiClientError> {
        self.inner.bidi_stream(request, path, body).await
    }
}

pub trait ToBoxedClient {
    fn boxed(self) -> BoxClient;
    /// Store this transport once and return a cloneable shared owner.
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
        ArcClient(Arc::new(BoxedClient::new(self)))
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
