//! Configuration values for API Related actions
//! gRPC for XMTPD and GATEWAY are the same on native/wasm
//! but differ for v3 node-go. this is because v3 node-go uses an envoy proxy for grpc-web.

use xmtp_common::{if_dev, if_local, if_native, if_wasm};

/// Localhost const
pub const LOCALHOST: &str = "http://localhost";

/// the max amount of data that can be sent in one gRPC call
/// should match GRPC_PAYLOAD_LIMIT in xmtp_api_grpc crate
pub const GRPC_PAYLOAD_LIMIT: usize = 1024 * 1024 * 25;

/// The timeout used by the multi-node client for:
/// - connect to the gateway and retrieve the list of nodes
/// - connect to nodes and perform a health check
pub const MULTI_NODE_TIMEOUT_MS: u64 = 30_000;

pub struct DeviceSyncUrls;
impl DeviceSyncUrls {
    pub const LOCAL_ADDRESS: &'static str = "http://0.0.0.0:5558";
    pub const DEV_ADDRESS: &'static str = "https://message-history.dev.ephemera.network";
    pub const PRODUCTION_ADDRESS: &'static str = "https://message-history.ephemera.network";
}

/// Docker URLS accessible from the Host
pub struct DockerUrls;
impl DockerUrls {
    /// Address to the locally running Anvil <https://getfoundry.sh/anvil/overview/>
    pub const ANVIL: &'static str = "http://localhost:8545";
}

/// Urls to the Grpc Backends
/// These URLS are rust-feature-flag aware, and will choose local or dev:
/// * if no feature is passed, uses local environment
/// * if `dev` feature is passed, uses dev environment
/// * if compiling for webassembly, uses the envoy/grpc-web variants of the local/dev urls
pub struct GrpcUrls;

// URLS for different networks (dev/local) are for switching all tests to that network
// on a specific feature flag.
if_dev! {
    impl GrpcUrls {
        pub const NODE: &'static str = GrpcUrlsDev::NODE;
        pub const XMTPD: &'static str = GrpcUrlsStaging::XMTPD;
        pub const GATEWAY: &'static str = GrpcUrlsStaging::GATEWAY;
    }
}

if_local! {
     impl GrpcUrls {
        pub const NODE: &'static str = GrpcUrlsLocal::NODE;
        pub const XMTPD: &'static str = GrpcUrlsLocal::XMTPD;
        pub const GATEWAY: &'static str = GrpcUrlsLocal::GATEWAY;
    }
}

/// GRPC URLS corresponding to local environments
pub struct GrpcUrlsLocal;
impl GrpcUrlsLocal {
    pub const XMTPD: &'static str = "http://localhost:5050";
    pub const GATEWAY: &'static str = "http://localhost:5052";
}

if_wasm! {
    impl GrpcUrlsLocal {
        pub const NODE: &'static str = "http://localhost:5557";
    }
}

if_native! {
    impl GrpcUrlsLocal {
        pub const NODE: &'static str = "http://localhost:5556";
    }
}

/// GRPC URLS corresponding to dev environments
pub struct GrpcUrlsDev;
impl GrpcUrlsDev {
    pub const XMTPD: &'static str = "https://grpc.testnet-dev.xmtp.network:443";
    pub const GATEWAY: &'static str = "https://payer.testnet-dev.xmtp.network:443";
}

if_wasm! {
    impl GrpcUrlsDev {
        pub const NODE: &'static str = "https://api.dev.xmtp.network:5558";
    }
}

if_native! {
     impl GrpcUrlsDev {
        pub const NODE: &'static str = "https://grpc.dev.xmtp.network:443";
    }
}

/// GRPC URLS corresponding to staging environments
pub struct GrpcUrlsStaging;
impl GrpcUrlsStaging {
    pub const XMTPD: &'static str = "https://grpc.testnet-staging.xmtp.network:443";
    pub const GATEWAY: &'static str = "https://payer.testnet-staging.xmtp.network:443";
}

if_wasm! {
    impl GrpcUrlsStaging {
        pub const NODE: &'static str = "https://api.dev.xmtp.network:5558";
    }
}

if_native! {
    impl GrpcUrlsStaging {
        pub const NODE: &'static str = "https://grpc.dev.xmtp.network:443";
    }
}

/// GRPC URLS corresponding to production environments
pub struct GrpcUrlsProduction;
impl GrpcUrlsProduction {
    pub const NODE: &'static str = "https://grpc.production.xmtp.network:443";
    pub const XMTPD: &'static str = "https://grpc.testnet.xmtp.network:443";
    pub const GATEWAY: &'static str = "https://payer.testnet.xmtp.network:443";
}

// URLs connected to toxiproxy
pub struct GrpcUrlsToxic;
impl GrpcUrlsToxic {
    /// URL to ToxiProxy version of NODE-GO
    pub const NODE: &'static str = "http://localhost:40010";
    /// URL to ToxiProxy version of NODE-GO Grpc Web
    pub const NODE_WEB: &'static str = "http://localhost:40020";
    /// URL to ToxiProxy version of XMTPD
    pub const XMTPD: &'static str = "http://localhost:40030";
    /// URL to ToxiProxy version of Payer Gateway
    pub const GATEWAY: &'static str = "http://localhost:40040";
    /// Url to ToxiProxy version of History Server
    pub const HISTORY_SERVER: &'static str = "http://localhost:40050";
    /// Url to ToxiProxy version of Anvil
    pub const ANVIL: &'static str = "http://localhost:40060";
}
