//! Api Client Traits

use prost::bytes::Bytes;
use std::borrow::Cow;
use thiserror::Error;

pub trait Endpoint {
    type Output: prost::Message + Default;

    fn http_endpoint(&self) -> Cow<'static, str>;

    fn grpc_endpoint(&self) -> Cow<'static, str>;

    fn body(&self) -> Result<Vec<u8>, BodyError>;
}

#[allow(async_fn_in_trait)]
pub trait Client {
    type Error: std::error::Error + Send + Sync + 'static;

    async fn request(
        &self,
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
#[allow(async_fn_in_trait)]
pub trait Query<T, C>
where
    C: Client,
{
    async fn query(&self, client: &C) -> Result<T, ApiError<C::Error>>;
}

// blanket Query implementation for a bare Endpoint
impl<E, T, C> Query<T, C> for E
where
    E: Endpoint,
    C: Client,
    T: TryFrom<E::Output, Error = crate::ConversionError>,
{
    async fn query(&self, client: &C) -> Result<T, ApiError<C::Error>> {
        let endpoint = if cfg!(feature = "http-api") {
            // use `Accept: application/x-protobuf`
            // to get response in protobuf instead of JSON for grpc-gateway
            // also ensure to set Content-Type header
            self.http_endpoint()
        } else {
            self.grpc_endpoint()
        };
        let request = http::Request::builder().uri(endpoint.as_ref());
        let rsp = client.request(request, self.body()?).await?;
        let rsp: E::Output = prost::Message::decode(rsp.into_body())?;
        Ok(rsp.try_into()?)
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
    Body(#[from] BodyError),
    #[error(transparent)]
    DecodeError(#[from] prost::DecodeError),
    #[error(transparent)]
    Conversion(#[from] crate::ConversionError),
}

#[derive(Debug, Error)]
pub enum BodyError {
    #[error("placeholder")]
    Placeholder,
}
