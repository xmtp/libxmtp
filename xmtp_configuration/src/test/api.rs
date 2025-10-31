//! Test constants/configuration for API

/// the max page size for queries
pub const MAX_PAGE_SIZE: u32 = 20;

pub struct ToxicUrls;

impl ToxicUrls {
    /// URL to ToxiProxy version of NODE-GO
    pub const NODE: &'static str = "http://localhost:40556";
    /// URL to ToxiProxy version of NODE-GO Grpc Web
    pub const NODE_WEB: &'static str = "http://localhost:40557";
    /// URL to ToxiProxy version of XMTPD
    pub const XMTPD: &'static str = "http://localhost:40050";
    /// URL to ToxiProxy version of Payer Gateway
    pub const GATEWAY: &'static str = "http://localhost:40052";
    /// Url to ToxiProxy version of History Server
    pub const HISTORY_SERVER: &'static str = "http://localhost:40558";
    /// Url to ToxiProxy version of Anvil
    pub const ANVIL: &'static str = "http://localhost:40545";
}

/// poor-mans dns docker resolution
/// Resolves a host docker address to an internal docker address
/// based on hard-coded port values.
pub fn localhost_to_internal(host_url: &str) -> url::Url {
    let mut url = url::Url::parse(host_url).unwrap();
    match url.port().unwrap() {
        5556 | 5555 => {
            url.set_host(Some("node")).unwrap() // the xmtp-go node
        }
        5050 | 5055 => {
            url.set_host(Some("xmtpd")).unwrap() // the xmtpd replication node
        }
        5052 => {
            url.set_host(Some("gateway")).unwrap() // the xmtpd gateway node
        }
        _ => panic!("unknown port value, missing port to internal docker translation?"),
    }

    url
}

/// Get the pre-determined toxiproxy port for this url
pub fn toxi_port(host_url: &str) -> u16 {
    let url = url::Url::parse(host_url).unwrap();
    match url.port().unwrap() {
        5556 => 21100, // node-go
        5555 => 21101, // http REST node-go
        5050 => 21102, // xmtpd
        5055 => 21103, // http REST xmtpd
        5052 => 21104, // xmtpd gateway
        _ => panic!("unknown port"),
    }
}
