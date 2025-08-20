use super::*;
use xmtp_configuration::GrpcUrls;
use xmtp_configuration::LOCALHOST;
use xmtp_proto::prelude::ApiBuilder;
use xmtp_proto::{api_client::XmtpTestClient, TestApiBuilder, ToxicProxies};

impl XmtpTestClient for GrpcClient {
    type Builder = ClientBuilder;

    fn create_local() -> Self::Builder {
        let mut client = GrpcClient::builder();
        if cfg!(target_arch = "wasm32") {
            client.set_host(GrpcUrls::NODE_WEB.into());
        }
        if cfg!(not(target_arch = "wasm32")) {
            client.set_host(GrpcUrls::NODE.into());
        }
        client.set_tls(false);
        client
    }

    fn create_local_d14n() -> Self::Builder {
        let mut client = GrpcClient::builder();
        if cfg!(target_arch = "wasm32") {
            client.set_host(GrpcUrls::XMTPD_WEB.into());
        }
        if cfg!(not(target_arch = "wasm32")) {
            client.set_host(GrpcUrls::XMTPD.into());
        }
        client.set_tls(false);
        client
    }

    fn create_local_payer() -> Self::Builder {
        let mut payer = GrpcClient::builder();
        if cfg!(target_arch = "wasm32") {
            payer.set_host(GrpcUrls::PAYER_WEB.into());
        }
        if cfg!(not(target_arch = "wasm32")) {
            payer.set_host(GrpcUrls::PAYER.into());
        }
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

