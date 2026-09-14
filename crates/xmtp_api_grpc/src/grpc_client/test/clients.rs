use crate::{ClientBuilder, GrpcClient};
xmtp_common::if_native! { use xmtp_common::toxiproxy; }
use xmtp_proto::{api_client::XmtpTestClient, prelude::NetConnectConfig};

use xmtp_configuration::{backend_test_toxic_url, backend_test_url};
fn build_client(host: &str) -> ClientBuilder {
    let mut client = GrpcClient::builder();
    client.set_host(host.parse().expect("test backend URL is valid"));
    client
}
pub struct BackendTestClient;
impl XmtpTestClient for BackendTestClient {
    type Builder = ClientBuilder;
    fn create() -> Self::Builder {
        build_client(&backend_test_url())
    }
}
pub struct ToxicBackendTestClient;
impl XmtpTestClient for ToxicBackendTestClient {
    type Builder = ClientBuilder;
    fn create() -> Self::Builder {
        build_client(&backend_test_toxic_url())
    }
}
xmtp_common::if_native! {
    use xmtp_proto::api_client::{ToxicProxies, ToxicTestClient};
    #[xmtp_common::async_trait]
    impl ToxicTestClient for ToxicBackendTestClient {
        async fn proxies() -> ToxicProxies {
            ToxicProxies::new([toxiproxy()
                .find_proxy("backend")
                .await
                .expect("backend proxy exists")])
        }
    }
}
