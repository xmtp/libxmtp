//! Api Client Traits

use crate::api_client::AggregateStats;
use http::{request, uri::PathAndQuery};
use prost::bytes::Bytes;
use std::borrow::Cow;
use xmtp_common::{BoxedRetry, RetryableError, retry_async};

#[cfg(any(test, feature = "test-utils"))]
pub mod mock;

mod query;

mod error;
pub use error::*;

pub trait HasStats {
    fn aggregate_stats(&self) -> AggregateStats;
}

pub trait Endpoint {
    type Output: prost::Message + Default;

    fn http_endpoint(&self) -> Cow<'static, str>;

    fn grpc_endpoint(&self) -> Cow<'static, str>;

    fn body(&self) -> Result<Bytes, BodyError>;
}

#[derive(thiserror::Error, Debug)]
pub enum MockE {}
use futures::{Future, Stream};
pub type BoxedClient = Box<
    dyn Client<
            Error = ApiClientError<MockE>,
            Stream = futures::stream::Once<Box<dyn Future<Output = ()>>>,
        >,
>;

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait Client {
    type Error: std::error::Error + Send + Sync + 'static;
    type Stream: futures::Stream<Item = Result<Bytes, Self::Error>>;

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

// query can return a Wrapper XmtpResponse<T> that implements both Future and Stream. If stream is used on singular response, just a stream of one item. This lets us re-use query for everything.
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait Query<T, C>
where
    C: Client + Send + Sync,
    T: Send,
{
    async fn query(&self, client: &C) -> Result<T, ApiClientError<C::Error>>;

    async fn query_retryable(
        &self,
        client: &C,
        retry: BoxedRetry,
    ) -> Result<T, ApiClientError<C::Error>>
    where
        C::Error: RetryableError,
    {
        retry_async!(retry, (async { self.query(client).await }))
    }

    async fn stream(
        &self,
        client: &C,
    ) -> Result<impl Stream<Item = Result<T, ApiClientError<C::Error>>>, ApiClientError<C::Error>>;
}
