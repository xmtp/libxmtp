//! Api Client Traits

use prost::bytes::Bytes;
use std::borrow::Cow;
use thiserror::Error;
use xmtp_common::{retry_async, retryable, BoxedRetry, RetryableError};

use crate::{ApiEndpoint, Code, ProtoError, XmtpApiError};

pub trait Endpoint {
    type Output: prost::Message + Default;

    fn http_endpoint(&self) -> Cow<'static, str>;

    fn grpc_endpoint(&self) -> Cow<'static, str>;

    fn body(&self) -> Result<Vec<u8>, BodyError>;
}
/*
/// Stream
pub struct Streaming<S, E>
where
    S: Stream<Item = Result<Bytes, ApiError<E>>>,
{
    inner: S,
}
*/

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait Client {
    type Error: std::error::Error + Send + Sync + 'static;
    type Stream: futures::Stream;

    async fn request(
        &self,
        request: http::request::Builder,
        body: Vec<u8>,
    ) -> Result<http::Response<Bytes>, ApiError<Self::Error>>;

    async fn stream(
        &self,
        request: http::request::Builder,
        body: Vec<u8>,
    ) -> Result<http::Response<Self::Stream>, ApiError<Self::Error>>;
}

// query can return a Wrapper XmtpResponse<T> that implements both Future and Stream. If stream is used on singular response, just a stream of one item. This lets us re-use query for everything.
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait Query<T, C>
where
    C: Client + Send + Sync,
    T: Send,
{
    async fn query(&self, client: &C) -> Result<T, ApiError<C::Error>>;

    async fn query_retryable(&self, client: &C, retry: BoxedRetry) -> Result<T, ApiError<C::Error>>
    where
        C::Error: RetryableError,
    {
        retry_async!(retry, (async { self.query(client).await }))
    }
}

// blanket Query implementation for a bare Endpoint
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<E, T, C> Query<T, C> for E
where
    E: Endpoint<Output = T> + Sync,
    C: Client + Sync + Send,
    T: Default + prost::Message,
    // TODO: figure out how to get conversions rightfigure out how to get conversions right
    // T: TryFrom<E::Output>,
    // ApiError<<C as Client>::Error>: From<<T as TryFrom<E::Output>>::Error>,
{
    async fn query(&self, client: &C) -> Result<T, ApiError<C::Error>> {
        let mut request = http::Request::builder();
        let endpoint = if cfg!(feature = "http-api") {
            request = request.header("Content-Type", "application/x-protobuf");
            request = request.header("Accept", "application/x-protobuf");
            self.http_endpoint()
        } else {
            self.grpc_endpoint()
        };
        let request = request.uri(endpoint.as_ref());
        let rsp = client.request(request, self.body()?).await?;
        let rsp: E::Output = prost::Message::decode(rsp.into_body())?;
        Ok(rsp)
    }
}

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
    #[error(transparent)]
    Http(#[from] http::Error),
    #[error(transparent)]
    Body(#[from] BodyError),
    #[error(transparent)]
    DecodeError(#[from] prost::DecodeError),
    #[error(transparent)]
    Conversion(#[from] crate::ConversionError),
    #[error(transparent)]
    ProtoError(#[from] ProtoError),
}

impl<E> XmtpApiError for ApiError<E>
where
    E: std::error::Error + Send + Sync + RetryableError + 'static,
{
    fn api_call(&self) -> Option<ApiEndpoint> {
        None
    }

    fn code(&self) -> Option<Code> {
        None
    }

    fn grpc_message(&self) -> Option<&str> {
        None
    }
}

impl<E> RetryableError for ApiError<E>
where
    E: RetryableError + std::error::Error + Send + Sync + 'static,
{
    fn is_retryable(&self) -> bool {
        use ApiError::*;
        match self {
            Client { source } => retryable!(source),
            Body(e) => retryable!(e),
            Http(_) => true,
            DecodeError(_) => false,
            Conversion(_) => false,
            ProtoError(_) => false
        }
    }
}

// Infallible errors by definition can never occur
impl<E: Send + Sync + std::error::Error> From<std::convert::Infallible> for ApiError<E> {
    fn from(_v: std::convert::Infallible) -> ApiError<E> {
        unreachable!()
    }
}

#[derive(Debug, Error)]
pub enum BodyError {
    #[error("placeholder")]
    Placeholder,
}

impl RetryableError for BodyError {
    fn is_retryable(&self) -> bool {
        false
    }
}
