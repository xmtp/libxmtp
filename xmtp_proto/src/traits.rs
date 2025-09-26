//! Api Client Traits

use crate::{api::XmtpStream, api_client::AggregateStats};
use http::{request, uri::PathAndQuery};
use prost::bytes::Bytes;
use std::borrow::Cow;

#[cfg(any(test, feature = "test-utils"))]
pub mod mock;

pub mod combinators;
mod error;
mod query;
pub mod stream;
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

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait Client {
    type Error: std::error::Error + Send + Sync + 'static;
    #[cfg(not(target_arch = "wasm32"))]
    type Stream: futures::Stream<Item = Result<Bytes, Self::Error>> + Send;
    #[cfg(target_arch = "wasm32")]
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

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait IsConnectedCheck {
    /// Check if a client is connected
    async fn is_connected(&self) -> bool;
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait Query<T, C, Specialized = ()>
where
    C: Client + Send + Sync,
    T: Send,
{
    async fn query(&mut self, client: &C) -> Result<T, ApiClientError<C::Error>>;
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait QueryStream<T, C, Specialized = ()>: Query<T, C, Specialized>
where
    C: Client + Send + Sync,
    T: Send,
{
    async fn stream(
        &mut self,
        client: &C,
    ) -> Result<XmtpStream<<C as Client>::Stream, T>, ApiClientError<C::Error>>;
}
