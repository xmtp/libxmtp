//! Api Client Traits

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

    // TODO: this T can be removed if we figure out how to drop unknown fields from proto messages
    // there must be a good way to do this with prost
    async fn request<T>(
        &self,
        request: http::request::Builder,
        uri: http::uri::Builder,
        body: Vec<u8>,
    ) -> Result<http::Response<T>, ApiError<Self::Error>>
    where
        T: Default + prost::Message + 'static,
        Self: Sized;

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
    T: Default + prost::Message + 'static,
    // TODO: figure out how to get conversions right
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
        let uri = http::uri::Uri::builder().path_and_query(endpoint.as_ref());
        let rsp = client.request::<T>(request, uri, self.body()?).await?;
        Ok(rsp.into_body())
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
            ProtoError(_) => false,
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

#[cfg(any(test, feature = "test-utils"))]
pub mod mock {
    use super::*;
    use crate::prelude::*;

    pub struct MockClient;
    pub struct MockStream;
    pub struct MockApiBuilder;
    impl ApiBuilder for MockApiBuilder {
        type Output = MockClient;
        type Error = MockError;

        fn set_libxmtp_version(&mut self, _version: String) -> Result<(), Self::Error> {
            Ok(())
        }
        fn set_app_version(&mut self, _version: String) -> Result<(), Self::Error> {
            Ok(())
        }
        fn set_host(&mut self, _host: String) {}
        fn set_payer(&mut self, _host: String) {}
        fn set_tls(&mut self, _tls: bool) {}
        async fn build(self) -> Result<Self::Output, Self::Error> {
            Ok(MockClient)
        }
    }

    #[derive(thiserror::Error, Debug)]
    pub enum MockError {}

    type Repeat = Box<dyn (FnMut() -> prost::bytes::Bytes)>;
    type MockStreamT = futures::stream::RepeatWith<Repeat>;
    #[cfg(not(target_arch = "wasm32"))]
    mockall::mock! {
        pub MockClient {}

        #[async_trait::async_trait]
        impl Client for MockClient {
            type Error = MockError;
            type Stream = MockStreamT;
            async fn request<T>(
                &self,
                request: http::request::Builder,
                uri: http::uri::Builder,
                body: Vec<u8>,
            ) -> Result<http::Response<T>, ApiError<MockError>> where Self: Sized, T: Default + prost::Message + 'static;

            async fn stream(
                &self,
                request: http::request::Builder,
                body: Vec<u8>,
            ) -> Result<http::Response<MockStreamT>, ApiError<MockError>>;
        }

        impl XmtpTestClient for MockClient {
            type Builder = MockApiBuilder;
            fn create_local() -> MockApiBuilder { MockApiBuilder }
            fn create_dev() -> MockApiBuilder { MockApiBuilder }
            fn create_local_payer() -> MockApiBuilder { MockApiBuilder }
            fn create_local_d14n() -> MockApiBuilder { MockApiBuilder }

        }
    }

    #[cfg(target_arch = "wasm32")]
    mockall::mock! {
        pub MockClient {}

        #[async_trait::async_trait(?Send)]
        impl Client for MockClient {
            type Error = MockError;
            type Stream = MockStreamT;
            async fn request<T>(
                &self,
                request: http::request::Builder,
                uri: http::uri::Builder,
                body: Vec<u8>,
            ) -> Result<http::Response<T>, ApiError<MockError>> where Self: Sized, T: Default + prost::Message + 'static;

            async fn stream(
                &self,
                request: http::request::Builder,
                body: Vec<u8>,
            ) -> Result<http::Response<MockStreamT>, ApiError<MockError>>;
        }

        impl XmtpTestClient for MockClient {
            type Builder = MockApiBuilder;
            fn create_local() -> () { () }
            fn create_dev() -> () { () }
            fn create_local_payer() -> () { () }
            fn create_local_d14n() -> () { () }

        }
    }
}
