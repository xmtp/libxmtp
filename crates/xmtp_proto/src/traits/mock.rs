use super::*;
use crate::{api_client::NetConnectConfig, prelude::*, types::AppVersion};

pub struct TestEndpoint;
impl Endpoint for TestEndpoint {
    type Output = ();

    fn grpc_endpoint(&self) -> std::borrow::Cow<'static, str> {
        Cow::Borrowed("")
    }

    fn body(&self) -> Result<bytes::Bytes, crate::api::BodyError> {
        Ok(vec![].into())
    }
}

pub struct MockStream;
pub struct MockApiBuilder;
impl ApiBuilder for MockApiBuilder {
    type Output = MockNetworkClient;
    type Error = MockError;

    fn build(self) -> Result<Self::Output, Self::Error> {
        Ok(MockNetworkClient::default())
    }
}

impl NetConnectConfig for MockApiBuilder {
    fn set_libxmtp_version(&mut self, _version: String) -> Result<(), Self::Error> {
        Ok(())
    }
    fn set_app_version(&mut self, _version: AppVersion) -> Result<(), Self::Error> {
        Ok(())
    }
    fn set_host(&mut self, _host: String) {}
    fn set_tls(&mut self, _tls: bool) {}
    fn rate_per_minute(&mut self, _limit: u32) {}

    fn port(&self) -> Result<Option<String>, Self::Error> {
        Ok(None)
    }

    fn host(&self) -> Option<&str> {
        None
    }

    fn set_retry(&mut self, _retry: xmtp_common::Retry) {}
}

#[derive(thiserror::Error, Debug)]
pub enum MockError {
    #[error("retryable mock error")]
    ARetryableError,
    #[error("non retryable mock error")]
    ANonRetryableError,
}

impl xmtp_common::RetryableError for MockError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::ARetryableError => true,
            Self::ANonRetryableError => false,
        }
    }
}

type Repeat = Box<dyn Send + FnMut() -> Result<prost::bytes::Bytes, MockError>>;
type MockStreamT = futures::stream::RepeatWith<Repeat>;

mockall::mock! {
    pub NetworkClient {}

    #[xmtp_common::async_trait]
    impl Client for NetworkClient {
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
            path: http::uri::PathAndQuery,
            body: Bytes,
        ) -> Result<http::Response<MockStreamT>, ApiClientError<MockError>>;

        fn fake_stream(&self) -> http::Response<MockStreamT>;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[xmtp_common::test]
    fn test_grpc_endpoint_returns_empty_string() {
        let endpoint = TestEndpoint;
        assert_eq!(endpoint.grpc_endpoint(), "");
    }
}
