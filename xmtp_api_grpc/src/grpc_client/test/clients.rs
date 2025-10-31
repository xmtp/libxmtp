use toxiproxy_rust::TOXIPROXY;
use xmtp_configuration::{GrpcUrls, GrpcUrlsDev, GrpcUrlsLocal, GrpcUrlsToxic};
use xmtp_proto::{
    api_client::{ToxicProxies, ToxicTestClient, XmtpTestClient},
    prelude::ApiBuilder,
};

use crate::{ClientBuilder, GrpcClient};

fn build_client(host: &str) -> ClientBuilder {
    let mut client = GrpcClient::builder();
    let url = url::Url::parse(host).unwrap();
    match url.scheme() {
        "https" => client.set_tls(true),
        _ => client.set_tls(false),
    }
    client.set_host(host.to_string());
    client
}
/// Client connected to the local/dev (feature flag) XmtpdClient
pub struct XmtpdClient;
impl XmtpTestClient for XmtpdClient {
    type Builder = ClientBuilder;

    fn create() -> Self::Builder {
        build_client(GrpcUrls::XMTPD)
    }
}

/// Client connected to the local/dev (feature flag) Payer Gateway
pub struct GatewayClient;
impl XmtpTestClient for GatewayClient {
    type Builder = ClientBuilder;

    fn create() -> Self::Builder {
        build_client(GrpcUrls::GATEWAY)
    }
}

/// A client connected to the local/dev (feature flag) Xmtp Node Go container
pub struct NodeGoClient;
impl XmtpTestClient for NodeGoClient {
    type Builder = ClientBuilder;

    fn create() -> Self::Builder {
        build_client(GrpcUrls::NODE)
    }
}

/// Client connected to xmtp-node-go on the dev network
pub struct DevNodeGoClient;
impl XmtpTestClient for DevNodeGoClient {
    type Builder = ClientBuilder;

    fn create() -> Self::Builder {
        build_client(GrpcUrlsDev::NODE)
    }
}

/// Client connected to xmtp-node-go on the dev network
pub struct DevGatewayClient;
impl XmtpTestClient for DevGatewayClient {
    type Builder = ClientBuilder;

    fn create() -> Self::Builder {
        build_client(GrpcUrlsDev::GATEWAY)
    }
}

/// Client connected to xmtp-node-go on the dev network
pub struct DevXmtpdClient;
impl XmtpTestClient for DevXmtpdClient {
    type Builder = ClientBuilder;

    fn create() -> Self::Builder {
        build_client(GrpcUrlsDev::XMTPD)
    }
}

/// Client connected to xmtp-node-go on the dev network
pub struct LocalXmtpdClient;
impl XmtpTestClient for LocalXmtpdClient {
    type Builder = ClientBuilder;

    fn create() -> Self::Builder {
        build_client(GrpcUrlsLocal::XMTPD)
    }
}

/// Client connected to xmtp-node-go on the dev network
pub struct LocalNodeGoClient;
impl XmtpTestClient for LocalNodeGoClient {
    type Builder = ClientBuilder;

    fn create() -> Self::Builder {
        build_client(GrpcUrlsLocal::NODE)
    }
}

/// Client connected to xmtp-node-go on the dev network
pub struct LocalGatewayClient;
impl XmtpTestClient for LocalGatewayClient {
    type Builder = ClientBuilder;

    fn create() -> Self::Builder {
        build_client(GrpcUrlsLocal::GATEWAY)
    }
}

/// Client connected to xmtp-node-go on the dev network
pub struct ToxicXmtpdClient;
impl XmtpTestClient for ToxicXmtpdClient {
    type Builder = ClientBuilder;

    fn create() -> Self::Builder {
        build_client(GrpcUrlsToxic::XMTPD)
    }
}

/// Client connected to xmtp-node-go on the dev network
pub struct ToxicNodeGoClient;
impl XmtpTestClient for ToxicNodeGoClient {
    type Builder = ClientBuilder;

    fn create() -> Self::Builder {
        build_client(GrpcUrlsToxic::NODE)
    }
}

/// Client connected to xmtp-node-go on the dev network
pub struct ToxicGatewayClient;
impl XmtpTestClient for ToxicGatewayClient {
    type Builder = ClientBuilder;

    fn create() -> Self::Builder {
        build_client(GrpcUrlsToxic::GATEWAY)
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl ToxicTestClient for ToxicXmtpdClient {
    async fn proxies() -> ToxicProxies {
        ToxicProxies::new([TOXIPROXY.find_proxy("xmtpd").await.unwrap()])
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl ToxicTestClient for ToxicNodeGoClient {
    async fn proxies() -> ToxicProxies {
        ToxicProxies::new([TOXIPROXY.find_proxy("node-go").await.unwrap()])
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl ToxicTestClient for ToxicGatewayClient {
    async fn proxies() -> ToxicProxies {
        ToxicProxies::new([TOXIPROXY.find_proxy("gateway").await.unwrap()])
    }
}
