use super::*;
use xmtp_configuration::{GrpcUrls, GrpcUrlsDev};
use xmtp_proto::prelude::{ApiBuilder, XmtpTestClient};

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
