use super::*;
use crate::{prelude::*, types::AppVersion, ToxicProxies};

pub struct MockStream;
pub struct MockApiBuilder;
impl ApiBuilder for MockApiBuilder {
    type Output = MockNetworkClient;
    type Error = MockError;
    fn set_libxmtp_version(&mut self, _version: String) -> Result<(), Self::Error> {
        Ok(())
    }
    fn set_app_version(&mut self, _version: AppVersion) -> Result<(), Self::Error> {
        Ok(())
    }
    fn set_host(&mut self, _host: String) {}
    fn set_payer(&mut self, _host: String) {}
    fn set_tls(&mut self, _tls: bool) {}
    fn rate_per_minute(&mut self, _limit: u32) {}

    fn port(&self) -> Result<Option<String>, Self::Error> {
        Ok(None)
    }

    async fn build(self) -> Result<Self::Output, Self::Error> {
        Ok(MockNetworkClient::default())
    }

    fn host(&self) -> Option<&str> {
        None
    }

    fn set_retry(&mut self, _retry: xmtp_common::Retry) {}
}

impl crate::TestApiBuilder for MockApiBuilder {
    async fn with_toxiproxy(&mut self) -> ToxicProxies {
        unimplemented!()
    }
}

#[derive(thiserror::Error, Debug)]
pub enum MockError {}

impl xmtp_common::RetryableError for MockError {
    fn is_retryable(&self) -> bool {
        false
    }
}

type Repeat = Box<dyn Send + FnMut() -> Result<prost::bytes::Bytes, MockError>>;
type MockStreamT = futures::stream::RepeatWith<Repeat>;
#[cfg(not(target_arch = "wasm32"))]
mockall::mock! {
    pub NetworkClient {}

    #[async_trait::async_trait]
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
    }
/*
    impl XmtpTestClient for NetworkClient {
        type Builder = MockApiBuilder;
        fn create_local() -> MockApiBuilder { MockApiBuilder }
        fn create_dev() -> MockApiBuilder { MockApiBuilder }
        fn create_payer() -> MockApiBuilder { MockApiBuilder }
        fn create_d14n() -> MockApiBuilder { MockApiBuilder }
    }
    */
}

#[cfg(target_arch = "wasm32")]
mockall::mock! {
    pub NetworkClient {}

    #[async_trait::async_trait(?Send)]
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
    }

    impl XmtpTestClient for NetworkClient {
        type Builder = MockApiBuilder;
        fn create_local() -> MockApiBuilder { MockApiBuilder }
        fn create_dev() -> MockApiBuilder { MockApiBuilder }
        fn create_payer() -> MockApiBuilder { MockApiBuilder }
        fn create_d14n() -> MockApiBuilder { MockApiBuilder }

    }
}
