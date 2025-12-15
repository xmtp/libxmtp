//! Api Client Traits

use crate::{
    api::{RetryQuery, V3Paged, XmtpStream, combinators::Ignore},
    api_client::{AggregateStats, ApiStats, IdentityStats},
};
use http::{request, uri::PathAndQuery};
use prost::bytes::Bytes;
use std::{borrow::Cow, sync::Arc};
use xmtp_common::{MaybeSend, MaybeSync, Retry};

xmtp_common::if_test! {
    pub mod mock;
}

mod boxed_client;
pub(super) mod combinators;
mod error;
mod query;
pub mod stream;
mod vector_clock;
pub use boxed_client::*;
pub use error::*;
pub use vector_clock::*;

pub trait HasStats {
    fn aggregate_stats(&self) -> AggregateStats;
    fn mls_stats(&self) -> ApiStats;
    fn identity_stats(&self) -> IdentityStats;
}

/// provides the necessary information for a backend API call.
/// Indicates the Output type
pub trait Endpoint<Specialized = ()>: MaybeSend + MaybeSync {
    type Output: MaybeSend + MaybeSync;
    fn grpc_endpoint(&self) -> Cow<'static, str>;

    fn body(&self) -> Result<Bytes, BodyError>;
}

pub trait EndpointExt<S>: Endpoint<S> {
    fn ignore_response(self) -> Ignore<Self>
    where
        Self: Sized + Endpoint<S>,
    {
        combinators::ignore(self)
    }

    fn v3_paged(self, cursor: Option<u64>) -> V3Paged<Self, <Self as Endpoint<S>>::Output>
    where
        Self: Sized + Endpoint<S>,
    {
        combinators::v3_paged(self, cursor)
    }

    fn retry(self) -> RetryQuery<Self>
    where
        Self: Sized + Endpoint<S>,
    {
        combinators::retry(self)
    }

    fn retry_with_strategy<St>(self, strategy: Retry<St>) -> RetryQuery<Self, St>
    where
        Self: Sized + Endpoint<S>,
    {
        combinators::retry_with_strategy(self, strategy)
    }
}

impl<S, E> EndpointExt<S> for E where E: Endpoint<S> {}

/// Trait indicating an [`Endpoint`] can be paged
/// paging will return a limited number of results
/// per request. a cursor is present indicating
/// the position in the total list of results
/// on the backend.
pub trait Pageable {
    /// set the cursor for this pageable endpoint
    fn set_cursor(&mut self, cursor: u64);
}

/// A client represents how a request body is formed and sent into
/// a backend. The client is protocol agnostic, a Client may
/// communicate with a backend over gRPC, JSON-RPC, HTTP-REST, etc.
/// `http::Response`'s are used in order to maintain a
/// common data format compatible with a wide variety of backends.
/// an http response is easily derived from a grpc, jsonrpc or rest api.
#[xmtp_common::async_trait]
pub trait Client: MaybeSend + MaybeSync {
    type Error: std::error::Error + MaybeSend + MaybeSync + 'static;

    type Stream: futures::Stream<Item = Result<Bytes, Self::Error>> + MaybeSend;

    async fn request(
        &self,
        request: request::Builder,
        path: PathAndQuery,
        body: Bytes,
    ) -> Result<http::Response<Bytes>, ApiClientError<Self::Error>>;

    async fn stream(
        &self,
        request: request::Builder,
        path: http::uri::PathAndQuery,
        body: Bytes,
    ) -> Result<http::Response<Self::Stream>, ApiClientError<Self::Error>>;

    /// start a "fake" stream that does not create a TCP connection and will always be pending
    fn fake_stream(&self) -> http::Response<Self::Stream>;
}

#[xmtp_common::async_trait]
impl<T: MaybeSend + MaybeSync + ?Sized> Client for &T
where
    T: Client,
{
    type Error = T::Error;

    type Stream = T::Stream;

    async fn request(
        &self,
        request: request::Builder,
        path: PathAndQuery,
        body: Bytes,
    ) -> Result<http::Response<Bytes>, ApiClientError<Self::Error>> {
        (**self).request(request, path, body).await
    }

    async fn stream(
        &self,
        request: request::Builder,
        path: http::uri::PathAndQuery,
        body: Bytes,
    ) -> Result<http::Response<Self::Stream>, ApiClientError<Self::Error>> {
        (**self).stream(request, path, body).await
    }

    fn fake_stream(&self) -> http::Response<Self::Stream> {
        (**self).fake_stream()
    }
}

#[xmtp_common::async_trait]
impl<T: MaybeSend + MaybeSync + ?Sized> Client for Box<T>
where
    T: Client,
{
    type Error = T::Error;

    type Stream = T::Stream;

    async fn request(
        &self,
        request: request::Builder,
        path: PathAndQuery,
        body: Bytes,
    ) -> Result<http::Response<Bytes>, ApiClientError<Self::Error>> {
        (**self).request(request, path, body).await
    }

    async fn stream(
        &self,
        request: request::Builder,
        path: http::uri::PathAndQuery,
        body: Bytes,
    ) -> Result<http::Response<Self::Stream>, ApiClientError<Self::Error>> {
        (**self).stream(request, path, body).await
    }

    fn fake_stream(&self) -> http::Response<Self::Stream> {
        (**self).fake_stream()
    }
}

#[xmtp_common::async_trait]
impl<T: MaybeSend + MaybeSync + ?Sized> Client for Arc<T>
where
    T: Client,
{
    type Error = T::Error;

    type Stream = T::Stream;

    async fn request(
        &self,
        request: request::Builder,
        path: PathAndQuery,
        body: Bytes,
    ) -> Result<http::Response<Bytes>, ApiClientError<Self::Error>> {
        (**self).request(request, path, body).await
    }

    async fn stream(
        &self,
        request: request::Builder,
        path: PathAndQuery,
        body: Bytes,
    ) -> Result<http::Response<Self::Stream>, ApiClientError<Self::Error>> {
        (**self).stream(request, path, body).await
    }

    /// start a "fake" stream that does not create a TCP connection and will always be pending
    fn fake_stream(&self) -> http::Response<Self::Stream> {
        (**self).fake_stream()
    }
}

#[xmtp_common::async_trait]
pub trait IsConnectedCheck: MaybeSend + MaybeSync {
    /// Check if a client is connected
    async fn is_connected(&self) -> bool;
}

/// Queries describe the way an endpoint is called.
/// these are extensions to the behavior of specific endpoints.
#[xmtp_common::async_trait]
pub trait Query<C: Client>: MaybeSend + MaybeSync {
    type Output: MaybeSend + MaybeSync;
    async fn query(&mut self, client: &C) -> Result<Self::Output, ApiClientError<C::Error>>;
}

#[xmtp_common::async_trait]
pub trait QueryRaw<C: Client>: MaybeSend + MaybeSync {
    async fn query_raw(&mut self, client: &C) -> Result<bytes::Bytes, ApiClientError<C::Error>>;
}

/// a companion to the [`Query`] trait, except for streaming calls.
/// Not every query combinator/extension will apply to both
/// steams and one-off calls (how do you 'page' a streaming api?),
/// so these traits are separated.
#[xmtp_common::async_trait]
pub trait QueryStream<T, C>
where
    C: Client,
{
    /// stream items from an endpoint
    /// [`QueryStreamExt::subscribe`] or [`crate::api::stream_as`] should be used to indicate
    /// the type of item in the stream.
    async fn stream(
        &mut self,
        client: &C,
    ) -> Result<XmtpStream<<C as Client>::Stream, T>, ApiClientError<C::Error>>;

    fn fake_stream(&mut self, client: &C) -> XmtpStream<<C as Client>::Stream, T>;
}

#[xmtp_common::async_trait]
pub trait QueryStreamExt<T, C: Client> {
    /// Subscribe to the endpoint, indicating the type of stream item with `R`
    async fn subscribe(
        &mut self,
        client: &C,
    ) -> Result<XmtpStream<<C as Client>::Stream, T>, ApiClientError<C::Error>>
    where
        T: Default + prost::Message + 'static;
}

#[xmtp_common::async_trait]
impl<T, C, E> QueryStreamExt<T, C> for E
where
    C: Client,
    E: Endpoint<Output = T>,
{
    async fn subscribe(
        &mut self,
        client: &C,
    ) -> Result<XmtpStream<<C as Client>::Stream, T>, ApiClientError<C::Error>>
    where
        T: Default + prost::Message + 'static,
    {
        self.stream(client).await
    }
}

#[cfg(test)]
mod test {
    use crate::api::{
        EndpointExt, Query,
        mock::{MockNetworkClient, TestEndpoint},
    };

    // test ensures these combinations can compile
    #[xmtp_common::test]
    async fn endpoints_can_be_chained() {
        let client = MockNetworkClient::new();
        std::mem::drop(TestEndpoint.ignore_response().retry().query(&client));
        std::mem::drop(TestEndpoint.retry().ignore_response().query(&client));
    }
}
