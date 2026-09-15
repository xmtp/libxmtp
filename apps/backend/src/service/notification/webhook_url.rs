//! Webhook host checks shared by registration and delivery.

use crate::config::push::HttpConfig;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

const DNS_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const SHARED_ADDRESS_NETWORK: u32 = u32::from_be_bytes([100, 64, 0, 0]);
const SHARED_ADDRESS_MASK: u32 = u32::from_be_bytes([255, 192, 0, 0]);

#[derive(Debug, thiserror::Error)]
#[error("webhook url is not allowed")]
pub(crate) struct WebhookUrlError;

/// Validate the URL and all resolved addresses at registration. Delivery must
/// repeat address classification and connect only to the checked addresses.
pub(crate) async fn validate(url: &str, config: &HttpConfig) -> Result<(), WebhookUrlError> {
    let parsed = url::Url::parse(url).map_err(|_| WebhookUrlError)?;
    if parsed.scheme() != "https" || !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(WebhookUrlError);
    }
    let host = parsed.host().ok_or(WebhookUrlError)?;
    if let Some(domains) = &config.allowed_domains
        && !domains.is_empty()
    {
        let name = host.to_string();
        if !domains.iter().any(|domain| matches_domain(&name, domain)) {
            return Err(WebhookUrlError);
        }
    }
    let port = parsed.port_or_known_default().ok_or(WebhookUrlError)?;
    let addresses: Vec<SocketAddr> = match host {
        url::Host::Ipv4(ip) => vec![SocketAddr::new(ip.into(), port)],
        url::Host::Ipv6(ip) => vec![SocketAddr::new(ip.into(), port)],
        url::Host::Domain(name) => {
            xmtp_common::time::timeout(DNS_TIMEOUT, tokio::net::lookup_host((name, port)))
                .await
                .map_err(|_| WebhookUrlError)?
                .map_err(|_| WebhookUrlError)?
                .collect()
        }
    };
    if addresses.is_empty()
        || (!config.allow_private_addresses
            && addresses.iter().any(|address| blocked(address.ip())))
    {
        return Err(WebhookUrlError);
    }
    Ok(())
}

/// A wildcard requires at least one complete label before the configured suffix.
pub(crate) fn matches_domain(host: &str, domain: &str) -> bool {
    let host = host.to_ascii_lowercase();
    let domain = domain.to_ascii_lowercase();
    match domain.strip_prefix('*') {
        Some(suffix) => host.len() > suffix.len() && host.ends_with(suffix),
        None => host == domain,
    }
}

fn blocked_v4(ip: Ipv4Addr) -> bool {
    ip.is_private()
        || (u32::from(ip) & SHARED_ADDRESS_MASK) == SHARED_ADDRESS_NETWORK
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_multicast()
}

/// Include mapped IPv4, unique-local IPv6, and IPv6 link-local ranges.
pub(crate) fn blocked(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => blocked_v4(ip),
        IpAddr::V6(ip) => {
            ip.to_ipv4_mapped().is_some_and(blocked_v4)
                || ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_unique_local()
                || ip.is_unicast_link_local()
                || ip.is_multicast()
        }
    }
}
