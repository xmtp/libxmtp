//! Api Client Traits

use crate::api_client::AggregateStats;
use http::{request, uri::PathAndQuery};
use prost::bytes::Bytes;
use std::borrow::Cow;
use thiserror::Error;
use xmtp_common::{retry_async, retryable, BoxedRetry, RetryableError};

use crate::{ApiEndpoint, ProtoError};

pub trait HasStats {
    fn aggregate_stats(&self) -> AggregateStats;
}

pub trait Endpoint {
    type Output: prost::Message + Default;

    fn http_endpoint(&self) -> Cow<'static, str>;

    fn grpc_endpoint(&self) -> Cow<'static, str>;

    fn body(&self) -> Result<Bytes, BodyError>;
}
/*
/// Stream
pub struct Streaming<S, E>
where
    S: Stream<Item = Result<Bytes, ApiClientError<E>>>,
{
    inner: S,
}
*/

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
    type Stream: futures::Stream;

    async fn request(
        &self,
        request: request::Builder,
        path: PathAndQuery,
        body: Bytes,
    ) -> Result<http::Response<Bytes>, ApiClientError<Self::Error>>;

    async fn stream(
        &self,
        request: request::Builder,
        body: Vec<u8>,
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
}

// blanket Query implementation for a bare Endpoint
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<E, T, C> Query<T, C> for E
where
    E: Endpoint<Output = T> + Sync,
    C: Client + Sync + Send,
    C::Error: std::error::Error,
    T: Default + prost::Message + 'static,
    // TODO: figure out how to get conversions right
    // T: TryFrom<E::Output>,
    // ApiError<<C as Client>::Error>: From<<T as TryFrom<E::Output>>::Error>,
{
    async fn query(&self, client: &C) -> Result<T, ApiClientError<C::Error>> {
        let mut request = http::Request::builder();
        let endpoint = if cfg!(any(feature = "http-api", target_arch = "wasm32")) {
            request = request.header("Content-Type", "application/x-protobuf");
            request = request.header("Accept", "application/x-protobuf");
            self.http_endpoint()
        } else {
            self.grpc_endpoint()
        };
        let path = http::uri::PathAndQuery::try_from(endpoint.as_ref())?;
        let rsp = client
            .request(request, path, self.body()?)
            .await
            .map_err(|e| e.endpoint(endpoint.into_owned()))?;
        let value: T = prost::Message::decode(rsp.into_body())?;
        Ok(value)
    }
}

impl<E> ApiClientError<E>
where
    E: std::error::Error + 'static,
{
    /*
        fn client(endpoint: String, source: E) -> Self {
            Self::ClientWithEndpoint { endpoint, source }
        }
    */
    pub fn new(endpoint: ApiEndpoint, source: E) -> Self {
        Self::ClientWithEndpoint {
            endpoint: endpoint.to_string(),
            source,
        }
    }

    /// add an endpoint to a ApiError::Client error
    pub fn endpoint(self, endpoint: String) -> Self {
        match self {
            Self::Client { source } => Self::ClientWithEndpoint { source, endpoint },
            v => v,
        }
    }
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ApiClientError<E: std::error::Error> {
    #[error(
        "api client at endpoint \"{}\" has error {}. \n {:?} \n",
        endpoint,
        source,
        stats
    )]
    ClientWithEndpointAndStats {
        endpoint: String,
        source: E,
        stats: AggregateStats,
    },
    #[error("API Error {}, \n {:?} \n", e, stats)]
    ErrorWithStats {
        e: Box<dyn RetryableError + Send + Sync>,
        stats: AggregateStats,
    },
    /// The client encountered an error.
    #[error("api client at endpoint \"{}\" has error {}", endpoint, source)]
    ClientWithEndpoint {
        endpoint: String,
        /// The client error.
        source: E,
    },
    #[error("client errored {}", source)]
    Client { source: E },
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
    #[error(transparent)]
    InvalidUri(#[from] http::uri::InvalidUri),
    #[error("{0}")]
    Other(Box<dyn RetryableError + Send + Sync>),
}

impl<E> RetryableError for ApiClientError<E>
where
    E: RetryableError + std::error::Error + 'static,
{
    fn is_retryable(&self) -> bool {
        use ApiClientError::*;
        match self {
            ClientWithEndpointAndStats { source, .. } => retryable!(source),
            ErrorWithStats { e, .. } => retryable!(e),
            Client { source } => retryable!(*source),
            ClientWithEndpoint { source, .. } => retryable!(source),
            Body(e) => retryable!(e),
            Http(_) => true,
            DecodeError(_) => false,
            Conversion(_) => false,
            ProtoError(_) => false,
            InvalidUri(_) => false,
            Other(r) => retryable!(r),
        }
    }
}

// Infallible errors by definition can never occur
impl<E: std::error::Error> From<std::convert::Infallible> for ApiClientError<E> {
    fn from(_v: std::convert::Infallible) -> ApiClientError<E> {
        unreachable!("Infallible errors can never occur")
    }
}

#[derive(Debug, Error)]
pub enum BodyError {
    #[error(transparent)]
    UninitializedField(#[from] derive_builder::UninitializedFieldError),
}

impl RetryableError for BodyError {
    fn is_retryable(&self) -> bool {
        false
    }
}

#[cfg(any(test, feature = "test-utils"))]
pub mod mock {
    use super::*;
    use crate::{prelude::*, ToxicProxies};

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
        fn rate_per_minute(&mut self, _limit: u32) {}

        fn port(&self) -> Result<Option<String>, Self::Error> {
            Ok(None)
        }

        fn build(self) -> Result<Self::Output, Self::Error> {
            Ok(MockClient)
        }

        fn host(&self) -> Option<&str> {
            None
        }
    }

    impl crate::TestApiBuilder for MockApiBuilder {
        async fn with_toxiproxy(&mut self) -> ToxicProxies {
            unimplemented!()
        }
    }

    #[derive(thiserror::Error, Debug)]
    pub enum MockError {}

    impl RetryableError for MockError {
        fn is_retryable(&self) -> bool {
            false
        }
    }

    type Repeat = Box<dyn FnMut() -> prost::bytes::Bytes>;
    type MockStreamT = futures::stream::RepeatWith<Repeat>;
    #[cfg(not(target_arch = "wasm32"))]
    mockall::mock! {
        pub MockClient {}

        #[async_trait::async_trait]
        impl Client for MockClient {
            type Error = MockError;
            type Stream = MockStreamT;
            async fn request(
                &self,
                request: http::request::Builder,
                path: http::uri::PathAndQuery,
                body: Bytes,
            ) -> Result<http::Response<Bytes>, ApiClientError<MockError>>;

            async fn stream(
                &self,
                request: http::request::Builder,
                body: Vec<u8>,
            ) -> Result<http::Response<MockStreamT>, ApiClientError<MockError>>;
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
            async fn request(
                &self,
                request: http::request::Builder,
                path: http::uri::PathAndQuery,
                body: Bytes,
            ) -> Result<http::Response<Bytes>, ApiClientError<MockError>>;

            async fn stream(
                &self,
                request: http::request::Builder,
                body: Vec<u8>,
            ) -> Result<http::Response<MockStreamT>, ApiClientError<MockError>>;
        }

        impl XmtpTestClient for MockClient {
            type Builder = MockApiBuilder;
            fn create_local() -> MockApiBuilder { MockApiBuilder }
            fn create_dev() -> MockApiBuilder { MockApiBuilder }
            fn create_local_payer() -> MockApiBuilder { MockApiBuilder }
            fn create_local_d14n() -> MockApiBuilder { MockApiBuilder }

        }
    }
}
