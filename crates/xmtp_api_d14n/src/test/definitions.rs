use crate::{BackendClient, TrackedStatsClient};
use xmtp_api_grpc::{ClientBuilder, GrpcClient, test::{BackendTestClient, ToxicBackendTestClient}};
use xmtp_proto::{api_client::{ApiBuilder, XmtpTestClient, ToxicProxies, ToxicTestClient}};

pub type TestClient = TrackedStatsClient<BackendClient<GrpcClient>>;
pub struct TestClientBuilder(ClientBuilder);
impl ApiBuilder for TestClientBuilder {
    type Output = TestClient;
    type Error = xmtp_api_grpc::error::GrpcBuilderError;
    fn build(self) -> Result<Self::Output, Self::Error> { Ok(TrackedStatsClient::new(BackendClient::new(self.0.build()?))) }
}
impl XmtpTestClient for TestClient {
    type Builder = TestClientBuilder;
    fn create() -> Self::Builder { TestClientBuilder(BackendTestClient::create()) }
}
pub struct ToxicTestClientCreator;
impl XmtpTestClient for ToxicTestClientCreator {
    type Builder = TestClientBuilder;
    fn create() -> Self::Builder { TestClientBuilder(ToxicBackendTestClient::create()) }
}
#[xmtp_common::async_trait]
impl ToxicTestClient for ToxicTestClientCreator {
    async fn proxies() -> ToxicProxies { ToxicBackendTestClient::proxies().await }
}
