/// The admin key all contracts are deployed with
pub const ANVIL_ADMIN_KEY: &str =
    "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
pub const SETTLEMENT_RPC_URL: &str = "todo";

// XMTP Docker Registry
/// Base URL for XMTP container registry
pub const XMTP_REGISTRY: &str = "ghcr.io/xmtp";

/// XMTPD
pub const DEFAULT_XMTPD_VERSION: &str = "main";
pub const DEFAULT_XMTPD_IMAGE: &str = "ghcr.io/xmtp/xmtpd";
pub const DEFAULT_XMTPD_CLI_IMAGE: &str = "ghcr.io/xmtp/xmtpd-cli";

/// Default contracts image (includes anvil + deployed XMTP contracts)
pub const DEFAULT_CONTRACTS_IMAGE: &str = "ghcr.io/xmtp/contracts";
/// Default contracts version tag
pub const DEFAULT_CONTRACTS_VERSION: &str = "v0.5.5";

/// Default Anvil RPC port
pub const ANVIL_PORT: u16 = 8545;
/// Container name for the Anvil chain
pub const ANVIL_CONTAINER_NAME: &str = "xnet-anvil";

// Redis configuration
/// Default Redis image
pub const DEFAULT_REDIS_IMAGE: &str = "redis:7-alpine";
/// Default Redis port
pub const REDIS_PORT: u16 = 6379;
/// Container name for Redis
pub const REDIS_CONTAINER_NAME: &str = "xnet-redis";

// PostgreSQL (ReplicationDb) configuration
/// Default PostgreSQL image
pub const DEFAULT_POSTGRES_IMAGE: &str = "postgres:16";
/// Default PostgreSQL port
pub const POSTGRES_PORT: u16 = 5432;
/// Container name for the replication database
pub const REPLICATION_DB_CONTAINER_NAME: &str = "xnet-replicationdb";
/// Default PostgreSQL password
pub const DEFAULT_POSTGRES_PASSWORD: &str = "xmtp";

// V3 Database (db) configuration - uses postgres:13 per docker-compose.yml
/// Default V3 database image
pub const DEFAULT_V3_DB_IMAGE: &str = "postgres:13";
/// Default V3 database port
pub const V3_DB_PORT: u16 = 5432;
/// Container name for V3 database
pub const V3_DB_CONTAINER_NAME: &str = "xnet-db";

// MLS Database (mlsdb) configuration - uses postgres:13 per docker-compose.yml
/// Default MLS database image
pub const DEFAULT_MLS_DB_IMAGE: &str = "postgres:13";
/// Default MLS database port
pub const MLS_DB_PORT: u16 = 5432;
/// Container name for MLS database
pub const MLS_DB_CONTAINER_NAME: &str = "xnet-mlsdb";

// History Server configuration
/// Default history server image
pub const DEFAULT_HISTORY_SERVER_IMAGE: &str = "ghcr.io/xmtp/message-history-server";
/// Default history server version tag
pub const DEFAULT_HISTORY_SERVER_VERSION: &str = "main";
/// Default history server port
pub const HISTORY_SERVER_PORT: u16 = 5558;
/// Container name for history server
pub const HISTORY_SERVER_CONTAINER_NAME: &str = "xnet-history-server";

// Validation Service configuration
/// Default validation service image
pub const DEFAULT_VALIDATION_IMAGE: &str = "ghcr.io/xmtp/mls-validation-service";
/// Default validation service version tag
pub const DEFAULT_VALIDATION_VERSION: &str = "main";
/// Default validation service gRPC port
pub const VALIDATION_PORT: u16 = 50051;
/// Container name for validation service
pub const VALIDATION_CONTAINER_NAME: &str = "xnet-validation";

// Node-Go configuration
/// Default node-go image
pub const DEFAULT_NODE_GO_IMAGE: &str = "ghcr.io/xmtp/node-go";
/// Default node-go version tag
pub const DEFAULT_NODE_GO_VERSION: &str = "main";
/// Default node-go API port
pub const NODE_GO_API_PORT: u16 = 5555;
/// Default node-go API HTTP port
pub const NODE_GO_API_HTTP_PORT: u16 = 5556;
/// Container name for node-go
pub const NODE_GO_CONTAINER_NAME: &str = "xnet-node";
/// Default node key for node-go
pub const DEFAULT_NODE_GO_NODE_KEY: &str =
    "8a30dcb604b0b53627a5adc054dbf434b446628d4bd1eccc681d223f0550ce67";

// Gateway configuration
/// Default gateway image
pub const DEFAULT_GATEWAY_IMAGE: &str = "ghcr.io/xmtp/xmtpd-gateway";
/// Default gateway version tag
pub const DEFAULT_GATEWAY_VERSION: &str = "main";
/// Gateway API port
pub const GATEWAY_PORT: u16 = 5052;
/// Container name for gateway
pub const GATEWAY_CONTAINER_NAME: &str = "xnet-gateway";

// ToxiProxy configuration
/// Default ToxiProxy image
pub const DEFAULT_TOXIPROXY_IMAGE: &str = "ghcr.io/shopify/toxiproxy:2.12.0";
/// ToxiProxy API port (for configuration)
pub const TOXIPROXY_API_PORT: u16 = 8555;
/// Container name for ToxiProxy
pub const TOXIPROXY_CONTAINER_NAME: &str = "xnet-toxiproxy";
/// Start of the port range for ToxiProxy proxy ports
pub const TOXIPROXY_PORT_RANGE_START: u16 = 8100;
/// End of the port range for ToxiProxy proxy ports (exclusive)
pub const TOXIPROXY_PORT_RANGE_END: u16 = 8150;
