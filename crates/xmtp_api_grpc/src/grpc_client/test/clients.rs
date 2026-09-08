use crate::{ClientBuilder, GrpcClient};
use toxiproxy_rust::TOXIPROXY;
use xmtp_proto::{
    api_client::{ToxicProxies, ToxicTestClient, XmtpTestClient},
    prelude::NetConnectConfig,
};

use xmtp_configuration::{BACKEND_TEST_TOXIC_URL, BACKEND_TEST_URL};
fn build_client(host: &str) -> ClientBuilder {
    let mut client = GrpcClient::builder();
    client.set_host(host.parse().expect("test backend URL is valid"));
    client
}
pub struct BackendTestClient;
impl XmtpTestClient for BackendTestClient {
    type Builder = ClientBuilder;
    fn create() -> Self::Builder {
        build_client(&std::env::var("XMTP_BACKEND_URL").unwrap_or_else(|_| BACKEND_TEST_URL.into()))
    }
}
pub struct ToxicBackendTestClient;
impl XmtpTestClient for ToxicBackendTestClient {
    type Builder = ClientBuilder;
    fn create() -> Self::Builder {
        build_client(BACKEND_TEST_TOXIC_URL)
    }
}
#[xmtp_common::async_trait]
impl ToxicTestClient for ToxicBackendTestClient {
    async fn proxies() -> ToxicProxies {
        ToxicProxies::new([TOXIPROXY
            .find_proxy("backend")
            .await
            .expect("backend proxy exists")])
    }
}
