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
impl GrpcUrls {
    pub const NODE: &'static str = "http://localhost:5556";
    pub const XMTPD: &'static str = "http://localhost:5050";
    pub const PAYER: &'static str = "http://localhost:5052";
    pub const NODE_DEV: &'static str = "https://grpc.dev.xmtp.network:443";
}

/// Internal Docker URLS Accessible from within docker network
/// useful for setting up proxies with toxiproxy
pub struct InternalDockerUrls;
impl InternalDockerUrls {
    pub const NODE: &'static str = "http://node:5556";
    pub const XMTPD: &'static str = "http://repnode:5050";
    pub const PAYER: &'static str = "http://gateway:5052";
}

/// URLS for the GRPC-Gateway <https://grpc-ecosystem.github.io/grpc-gateway/>
pub struct HttpGatewayUrls;
impl HttpGatewayUrls {
    pub const NODE: &'static str = "http://localhost:5555";
    pub const XMTPD: &'static str = "http://localhost:5055";
    pub const PAYER: &'static str = "http://localhost:5052";
    pub const NODE_DEV: &'static str = "https://dev.xmtp.network:443";
    pub const NODE_PRODUCTION: &'static str = "https://production.xmtp.network";
}

/// Endpoints for the GRPC-Gateway: <https://grpc-ecosystem.github.io/grpc-gateway/>
pub struct RestApiEndpoints;
impl RestApiEndpoints {
    pub const FETCH_KEY_PACKAGES: &'static str = "/mls/v1/fetch-key-packages";
    pub const GET_IDENTITY_UPDATES: &'static str = "/identity/v1/get-identity-updates";
    pub const GET_INBOX_IDS: &'static str = "/identity/v1/get-inbox-ids";
    pub const PUBLISH_COMMIT_LOG: &'static str = "/mls/v1/batch-publish-commit-log";
    pub const PUBLISH_IDENTITY_UPDATE: &'static str = "/identity/v1/publish-identity-update";
    pub const QUERY_COMMIT_LOG: &'static str = "/mls/v1/batch-query-commit-log";
    pub const QUERY_GROUP_MESSAGES: &'static str = "/mls/v1/query-group-messages";
    pub const QUERY_WELCOME_MESSAGES: &'static str = "/mls/v1/query-welcome-messages";
    pub const REGISTER_INSTALLATION: &'static str = "/mls/v1/register-installation";
    pub const SEND_GROUP_MESSAGES: &'static str = "/mls/v1/send-group-messages";
    pub const SEND_WELCOME_MESSAGES: &'static str = "/mls/v1/send-welcome-messages";
    pub const SUBSCRIBE_GROUP_MESSAGES: &'static str = "/mls/v1/subscribe-group-messages";
    pub const SUBSCRIBE_WELCOME_MESSAGES: &'static str = "/mls/v1/subscribe-welcome-messages";
    pub const UPLOAD_KEY_PACKAGE: &'static str = "/mls/v1/upload-key-package";
    pub const VERIFY_SMART_CONTRACT_WALLET_SIGNATURES: &'static str =
        "/identity/v1/verify-smart-contract-wallet-signatures";
}
