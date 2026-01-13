//! DNS setup helper for configuring host to resolve *.xmtpd.local
//!
//! Provides instructions and utilities for configuring the host system
//! to use CoreDNS for resolving XMTP local hostnames.

/// Check if DNS is configured correctly for *.xmtpd.local
///
/// Attempts to resolve a test hostname and returns `Ok(())` if it resolves
/// to 127.0.0.1, or an error describing the failure.
pub async fn check_dns_configured() -> color_eyre::eyre::Result<()> {
    use color_eyre::eyre::eyre;
    match tokio::net::lookup_host("node0.xmtpd.local:80").await {
        Ok(mut addrs) => match addrs.next() {
            Some(addr) if addr.ip().to_string() == "127.0.0.1" => Ok(()),
            Some(addr) => Err(eyre!(
                "node0.xmtpd.local resolved to {} instead of 127.0.0.1",
                addr.ip()
            )),
            None => Err(eyre!(
                "node0.xmtpd.local resolved but returned no addresses"
            )),
        },
        Err(e) => Err(eyre!("DNS lookup for node0.xmtpd.local failed: {}", e)),
    }
}
