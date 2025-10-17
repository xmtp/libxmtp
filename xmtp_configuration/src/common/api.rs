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

/// Urls to the Grpc Backends
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
            pub const NODE: &'static str = "https://grpc.dev.xmtp.network:443";
            pub const XMTPD: &'static str = "https://localhost:5050";
            pub const GATEWAY: &'static str = "https://localhost:5052";
        }
    }

    xmtp_common::if_local! {
         impl GrpcUrls {
            pub const NODE: &'static str = "http://localhost:5556";
            pub const XMTPD: &'static str = "http://localhost:5050";
            pub const GATEWAY: &'static str = "http://localhost:5052";
        }
    }
}

impl GrpcUrls {
    pub const NODE_DEV: &'static str = "https://grpc.dev.xmtp.network:443";
}
/// Internal Docker URLS Accessible from within docker network
/// useful for setting up proxies with toxiproxy
pub struct InternalDockerUrls;
impl InternalDockerUrls {
    pub const NODE: &'static str = "http://node:5556";
    pub const XMTPD: &'static str = "http://xmtpd:5050";
    pub const GATEWAY: &'static str = "http://gateway:5052";
}
