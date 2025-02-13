//! Api Client Traits

use prost::bytes::Bytes;
use std::borrow::Cow;
use thiserror::Error;

trait Endpoint {
    fn http_endpoint(&self) -> Cow<'static, str>;

    fn grpc_endpoint(&self) -> Cow<'static, str>;

    fn body(&self) -> Result<Vec<u8>, BodyError>;
}

#[allow(async_fn_in_trait)]
pub trait Client {
    type Error: std::error::Error + Send + Sync + 'static;

    async fn request(
        &mut self,
        request: http::request::Builder,
        body: Vec<u8>,
    ) -> Result<http::Response<Bytes>, ApiError<Self::Error>>;

    async fn stream(
        &self,
        request: http::request::Builder,
        body: Vec<u8>,
    ) -> Result<http::Response<Bytes>, ApiError<Self::Error>>;
}

// query can return a Wrapper XmtpResponse<T> that implements both Future and Stream. If stream is used on singular response, just a stream of one item. This lets us re-use query for everything.
trait Query<T, C>
where
    C: Client,
{
    async fn query(&self, client: &C) -> Result<T, ApiError<C::Error>>;
}

/*
// blanket Query implementation for a bare Endpoint
impl<E, T, C> Query<T, C> for E
where
    E: Endpoint,
    T: TryInto,
    C: Client,
{
    /* ... */
}
*/

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ApiError<E>
where
    E: std::error::Error + Send + Sync + 'static,
{
    /// The client encountered an error.
    #[error("client error: {}", source)]
    Client {
        /// The client error.
        source: E,
    },
}

#[derive(Debug, Error)]
pub enum BodyError {
    #[error("placeholder")]
    Placeholder,
}
