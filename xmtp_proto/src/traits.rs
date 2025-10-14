//! Api Client Traits

use crate::{
    api::{RetryQuery, V3Paged, XmtpStream, combinators::Ignore},
    api_client::AggregateStats,
};
use http::{request, uri::PathAndQuery};
use prost::bytes::Bytes;
use std::borrow::Cow;
use xmtp_common::{MaybeSend, Retry};

#[cfg(any(test, feature = "test-utils"))]
pub mod mock;

pub mod buffered_stream;
pub mod combinators;
mod error;
mod query;
pub mod stream;
pub use error::*;

pub trait HasStats {
    fn aggregate_stats(&self) -> AggregateStats;
}

/// provides the necessary information for a backend API call.
/// Indicates the Output type
pub trait Endpoint<Specialized = ()>: Send + Sync {
    type Output: Send + Sync;
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

pub trait Pageable {
    /// set the cursor for this pageable endpoint
    fn set_cursor(&mut self, cursor: u64);
}

#[derive(thiserror::Error, Debug)]
pub enum MockE {}
use futures::Future;
pub type BoxedClient = Box<
    dyn Client<
            Error = ApiClientError<MockE>,
            Stream = futures::stream::Once<Box<dyn Future<Output = ()>>>,
        >,
>;

/// A client represents how a request body is formed and sent into
/// a backend. The client is protocol agnostic, a Client may
/// communicate with a backend over gRPC, JSON-RPC, HTTP-REST, etc.
/// `http::Response`'s are used in order to maintain a
/// common data format compatible with a wide variety of backends.
/// an http response is easily derived from a grpc, jsonrpc or rest api.
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait Client: Send + Sync {
    type Error: std::error::Error + Send + Sync + 'static;

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
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait IsConnectedCheck {
    /// Check if a client is connected
    async fn is_connected(&self) -> bool;
}

/// Queries describe the way an endpoint is called.
/// these are extensions to the behavior of specific endpoints.
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait Query<C: Client>: Send + Sync {
    type Output: Send + Sync;
    async fn query(&mut self, client: &C) -> Result<Self::Output, ApiClientError<C::Error>>;
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait QueryRaw<C: Client>: Send + Sync {
    async fn query_raw(&mut self, client: &C) -> Result<bytes::Bytes, ApiClientError<C::Error>>;
}

/// a companion to the [`Query`] trait, except for streaming calls.
/// Not every query combinator/extension will apply to both
/// steams and one-off calls (how do you 'page' a streaming api?),
/// so these traits are separated.
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
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
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait QueryStreamExt<C: Client> {
    /// Subscribe to the endpoint, indicating the type of stream item with `R`
    async fn subscribe<R>(
        &mut self,
        client: &C,
    ) -> Result<XmtpStream<<C as Client>::Stream, R>, ApiClientError<C::Error>>
    where
        R: Default + prost::Message + 'static;
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<C, E> QueryStreamExt<C> for E
where
    C: Client + Send + Sync,
    E: Endpoint + Send + Sync,
    C: Client + Sync + Send,
    C::Error: std::error::Error,
{
    async fn subscribe<R>(
        &mut self,
        client: &C,
    ) -> Result<XmtpStream<<C as Client>::Stream, R>, ApiClientError<C::Error>>
    where
        R: Default + prost::Message + 'static,
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
