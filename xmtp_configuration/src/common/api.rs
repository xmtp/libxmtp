//! Configuration values for API Related actions
pub const LOCALHOST: &str = "http://localhost";

/// the max amount of data that can be sent in one gRPC call
/// should match GRPC_PAYLOAD_LIMIT in xmtp_api_grpc crate
pub const GRPC_PAYLOAD_LIMIT: usize = 1024 * 1024 * 25;

pub struct DeviceSyncUrls;
impl DeviceSyncUrls {
    pub const LOCAL_ADDRESS: &'static str = "http://0.0.0.0:5558";
    pub const DEV_ADDRESS: &'static str = "https://message-history.dev.ephemera.network";
    pub const PRODUCTION_ADDRESS: &'static str = "https://message-history.ephemera.network";
}

/// Docker URLS accessible from the Host
pub struct DockerUrls;
impl DockerUrls {
    /// Address to locally-running toxiproxy <https://github.com/Shopify/toxiproxy>
    pub const TOXIPROXY: &'static str = "0.0.0.0:8474";
    /// Address to the locally running Anvil <https://getfoundry.sh/anvil/overview/>
    pub const ANVIL: &'static str = "http://localhost:8545";
}

/// poor mans docker dns, for testing only
/// converts internal docker hosts to localhost
/// panics if host fails to set to localhost on [`url::Url`]
pub fn internal_to_localhost(host_url: &str) -> String {
    let mut url = url::Url::parse(host_url).unwrap();
    match url.domain().unwrap() {
        "repnode" | "node" | "gateway" => {
            url.set_host(Some("localhost")).unwrap();
        }
        _ => (),
    }
    url.into()
}

/// Urls to the Grpc Backends
/// These URLS are rust-feature-flag aware, and will choose local or dev:
/// * if no feature is passed, uses local environment
/// * if `dev` feature is passed, uses dev environment
/// * if compiling for webassembly, uses the envoy/grpc-web variants of the local/dev urls
pub struct GrpcUrls;

xmtp_common::if_wasm! {
    xmtp_common::if_dev! {
        impl GrpcUrls {
            pub const NODE: &'static str = "https://api.dev.xmtp.network:5558";
            pub const XMTPD: &'static str = "https://grpc.testnet-staging.xmtp.network:5558/xmtpd";
            pub const GATEWAY: &'static str = "https://payer.testnet-staging.xmtp.network:5558/payer";
        }
    }

    xmtp_common::if_local! {
        impl GrpcUrls {
            pub const NODE: &'static str = "http://localhost:5557";
            pub const XMTPD: &'static str = "http://localhost:5051/xmtpd";
            pub const GATEWAY: &'static str = "http://localhost:5051/gateway";
        }
    }
}

// URLS for different networks (dev/local) are for switching all tests to that network
// on a specific feature flag.
xmtp_common::if_native! {
    xmtp_common::if_dev! {
        impl GrpcUrls {
            pub const NODE: &'static str = GrpcUrlsStaging::NODE;
            pub const XMTPD: &'static str = GrpcUrlsStaging::XMTPD;
            pub const GATEWAY: &'static str = GrpcUrlsStaging::GATEWAY;
        }
    }

    xmtp_common::if_local! {
         impl GrpcUrls {
            pub const NODE: &'static str = GrpcUrlsLocal::NODE;
            pub const XMTPD: &'static str = GrpcUrlsLocal::XMTPD;
            pub const GATEWAY: &'static str = GrpcUrlsLocal::GATEWAY;
        }
    }
}

/// GRPC URLS corresponding to local environments
pub struct GrpcUrlsLocal;
impl GrpcUrlsLocal {
    pub const NODE: &'static str = "http://localhost:5556";
    pub const XMTPD: &'static str = "http://localhost:5050";
    pub const GATEWAY: &'static str = "http://localhost:5052";
}

/// GRPC URLS corresponding to dev environments
pub struct GrpcUrlsDev;
impl GrpcUrlsDev {
    pub const NODE: &'static str = "https://grpc.dev.xmtp.network:443";
    pub const XMTPD: &'static str = "https://grpc.testnet-dev.xmtp.network:443";
    pub const GATEWAY: &'static str = "https://payer.testnet-dev.xmtp.network:443";
}

/// GRPC URLS corresponding to staging environments
pub struct GrpcUrlsStaging;
impl GrpcUrlsStaging {
    pub const NODE: &'static str = "https://grpc.dev.xmtp.network:443";
    pub const XMTPD: &'static str = "https://grpc.testnet-staging.xmtp.network:443";
    pub const GATEWAY: &'static str = "https://payer.testnet-staging.xmtp.network:443";
}

/// GRPC URLS corresponding to production environments
pub struct GrpcUrlsProduction;
impl GrpcUrlsProduction {
    pub const NODE: &'static str = "https://grpc.production.xmtp.network:443";
    pub const XMTPD: &'static str = "https://grpc.testnet.xmtp.network:443";
    pub const GATEWAY: &'static str = "https://payer.testnet.xmtp.network:443";
}

/// Internal Docker URLS Accessible from within docker network
/// useful for setting up proxies with toxiproxy
pub struct InternalDockerUrls;
impl InternalDockerUrls {
    pub const NODE: &'static str = "http://node:5556";
    pub const XMTPD: &'static str = "http://xmtpd:5050";
    pub const GATEWAY: &'static str = "http://gateway:5052";
}
