use super::*;
use xmtp_configuration::GrpcUrls;
use xmtp_configuration::LOCALHOST;
use xmtp_proto::prelude::ApiBuilder;
use xmtp_proto::{TestApiBuilder, ToxicProxies, api_client::XmtpTestClient};

impl XmtpTestClient for GrpcClient {
    type Builder = ClientBuilder;

    fn create_local() -> Self::Builder {
        let mut client = GrpcClient::builder();
        let url = url::Url::parse(GrpcUrls::NODE).unwrap();
        match url.scheme() {
            "https" => client.set_tls(true),
            _ => client.set_tls(false),
        }
        client.set_host(GrpcUrls::NODE.into());
        client
    }

    fn create_d14n() -> Self::Builder {
        let mut client = GrpcClient::builder();
        let url = url::Url::parse(GrpcUrls::XMTPD).unwrap();
        match url.scheme() {
            "https" => client.set_tls(true),
            _ => client.set_tls(false),
        }
        client.set_host(GrpcUrls::XMTPD.into());
        client
    }

    fn create_gateway() -> Self::Builder {
        let mut payer = GrpcClient::builder();
        let url = url::Url::parse(GrpcUrls::GATEWAY).unwrap();
        match url.scheme() {
            "https" => payer.set_tls(true),
            _ => payer.set_tls(false),
        }
        payer.set_host(GrpcUrls::GATEWAY.into());
        payer.set_tls(false);
        payer
    }

    fn create_dev() -> Self::Builder {
        let mut client = GrpcClient::builder();
        client.set_host(GrpcUrls::NODE_DEV.into());
        client.set_tls(true);
        client
    }
}

impl TestApiBuilder for super::ClientBuilder {
    async fn with_toxiproxy(&mut self) -> ToxicProxies {
        let proxy = xmtp_proto::init_toxi(&[self.host().unwrap()]).await;
        self.set_host(format!("{LOCALHOST}:{}", proxy.port(0)));
        proxy
    }
}
