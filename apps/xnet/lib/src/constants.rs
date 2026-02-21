//! Constants for xnet services, organized by service using zero-sized struct namespacing.

/// XMTP Docker Registry base URL
pub const XMTP_REGISTRY: &str = "ghcr.io/xmtp";

/// Maximum number of XMTPD nodes supported (for pre-allocating hostnames)
pub const MAX_XMTPD_NODES: usize = 50;

/// Shared PostgreSQL password across all database services
pub const POSTGRES_PASSWORD: &str = "xmtp";

// --- Anvil (local chain with deployed XMTP contracts) ---

pub struct Anvil;
impl Anvil {
    pub const ADMIN_KEY: &str =
        "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
    pub const SETTLEMENT_RPC_URL: &str = "http://xnet-anvil:8545";
    pub const IMAGE: &str = "ghcr.io/xmtp/contracts";
    pub const VERSION: &str = "main";
    pub const PORT: u16 = 8545;
    pub const CONTAINER_NAME: &str = "xnet-anvil";
}

// --- XMTPD ---

pub struct Xmtpd;
impl Xmtpd {
    pub const IMAGE: &str = "ghcr.io/xmtp/xmtpd";
    pub const CLI_IMAGE: &str = "ghcr.io/xmtp/xmtpd-cli";
    pub const VERSION: &str = "sha-695b07e";
    pub const GRPC_PORT: u16 = 5050;
    pub const NODE_ID_INCREMENT: u32 = 100;
}

// --- Gateway ---

pub struct Gateway;
impl Gateway {
    pub const IMAGE: &str = "ghcr.io/xmtp/xmtpd-gateway";
    pub const VERSION: &str = "sha-695b07e";
    pub const PORT: u16 = 5052;
    pub const CONTAINER_NAME: &str = "xnet-gateway";
    pub const PRIVATE_KEY: &str =
        "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
}

// --- Redis ---

pub struct Redis;
impl Redis {
    pub const IMAGE: &str = "redis:7-alpine";
    pub const PORT: u16 = 6379;
    pub const CONTAINER_NAME: &str = "xnet-redis";
}

// --- ReplicationDb (per-node PostgreSQL) ---

pub struct ReplicationDb;
impl ReplicationDb {
    pub const IMAGE: &str = "postgres:16";
    pub const PORT: u16 = 5432;
    pub const CONTAINER_NAME: &str = "xnet-replicationdb";
    pub const PASSWORD: &str = "xmtp";
}

// --- V3 Database ---

pub struct V3Db;
impl V3Db {
    pub const IMAGE: &str = "postgres:13";
    pub const PORT: u16 = 5433;
    pub const CONTAINER_NAME: &str = "xnet-db";
}

// --- MLS Database ---

pub struct MlsDb;
impl MlsDb {
    pub const IMAGE: &str = "postgres:13";
    pub const PORT: u16 = 5434;
    pub const CONTAINER_NAME: &str = "xnet-mlsdb";
}

// --- History Server ---

pub struct HistoryServer;
impl HistoryServer {
    pub const IMAGE: &str = "ghcr.io/xmtp/message-history-server";
    pub const VERSION: &str = "main";
    pub const PORT: u16 = 5558;
    pub const CONTAINER_NAME: &str = "xnet-history-server";
}

// --- Validation Service ---

pub struct Validation;
impl Validation {
    pub const IMAGE: &str = "ghcr.io/xmtp/mls-validation-service";
    pub const VERSION: &str = "main";
    pub const PORT: u16 = 50051;
    pub const CONTAINER_NAME: &str = "xnet-validation";
}

// --- Node-Go (V3) ---

pub struct NodeGo;
impl NodeGo {
    pub const IMAGE: &str = "ghcr.io/xmtp/node-go";
    pub const VERSION: &str = "main";
    pub const API_PORT: u16 = 5556;
    pub const API_HTTP_PORT: u16 = 5555;
    pub const CONTAINER_NAME: &str = "xnet-node";
    pub const NODE_KEY: &str = "8a30dcb604b0b53627a5adc054dbf434b446628d4bd1eccc681d223f0550ce67";
}

// --- ToxiProxy ---

pub struct ToxiProxy;
impl ToxiProxy {
    pub const IMAGE: &str = "ghcr.io/shopify/toxiproxy";
    pub const VERSION: &str = "2.12.0";
    pub const API_PORT: u16 = 8555;
    pub const CONTAINER_NAME: &str = "xnet-toxiproxy";
    pub const STATIC_PORT_RANGE: (u16, u16) = (8100, 8120);
    pub const XMTPD_PORT_RANGE: (u16, u16) = (8150, 8200);
}

// --- Otterscan ---

pub struct Otterscan;
impl Otterscan {
    pub const IMAGE: &str = "otterscan/otterscan";
    pub const VERSION: &str = "latest";
    pub const PORT: u16 = 80;
    pub const EXTERNAL_PORT: u16 = 5100;
    pub const CONTAINER_NAME: &str = "xnet-otterscan";
}

// --- Prometheus ---

pub struct Prometheus;
impl Prometheus {
    pub const IMAGE: &str = "prom/prometheus";
    pub const VERSION: &str = "latest";
    pub const PORT: u16 = 9090;
    pub const EXTERNAL_PORT: u16 = 9090;
    pub const CONTAINER_NAME: &str = "xnet-prometheus";
    /// Default xmtpd metrics port (used for scrape targets)
    pub const METRICS_PORT: u16 = 8008;
}

// --- Grafana ---

pub struct Grafana;
impl Grafana {
    pub const IMAGE: &str = "ghcr.io/xmtp/grafana-xmtpd";
    pub const VERSION: &str = "latest";
    pub const PORT: u16 = 3000;
    pub const EXTERNAL_PORT: u16 = 3000;
    pub const CONTAINER_NAME: &str = "xnet-grafana";
}

// --- CoreDNS ---

pub struct CoreDns;
impl CoreDns {
    pub const IMAGE: &str = "coredns/coredns";
    pub const VERSION: &str = "1.11.1";
    pub const PORT: u16 = 5354;
    pub const CONTAINER_NAME: &str = "xnet-coredns";
}

// --- Traefik ---

pub struct Traefik;
impl Traefik {
    pub const IMAGE: &str = "traefik";
    pub const VERSION: &str = "v3.2";
    pub const HTTP_PORT: u16 = 80;
    pub const DASHBOARD_PORT: u16 = 8080;
    pub const CONTAINER_NAME: &str = "xnet-traefik";
}
