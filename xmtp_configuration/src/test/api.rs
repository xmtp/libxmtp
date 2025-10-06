//! Test constants/configuration for API

/// the max page size for queries
pub const MAX_PAGE_SIZE: u32 = 20;

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
            url.set_host(Some("repnode")).unwrap() // the xmtpd replication node
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
        5050 => 21102, // repnode
        5055 => 21103, // http REST repnode
        5052 => 21104, // xmtpd gateway
        _ => panic!("unknown port"),
    }
}
