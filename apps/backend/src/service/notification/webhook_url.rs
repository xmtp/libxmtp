//! Webhook host checks shared by registration and delivery.

use crate::config::push::HttpConfig;
use std::net::{IpAddr, Ipv6Addr, SocketAddr, ToSocketAddrs};
use std::sync::Arc;
use tokio::sync::Semaphore;
use xmtp_common::address::{NAT64_DISCOVERY_NAME, discover_nat64};

const DNS_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const SITE_LOCAL_NETWORK: u16 = 0xfec0;
const SITE_LOCAL_MASK: u16 = 0xffc0;
const MAX_LOOKUPS: usize = 32;

/// Bounds the blocking threads that registration lookups hold.
static LOOKUPS: Semaphore = Semaphore::const_new(MAX_LOOKUPS);

#[derive(Debug, PartialEq, thiserror::Error)]
pub(crate) enum WebhookUrlError {
    #[error("webhook url is not allowed")]
    NotAllowed,
    #[error("webhook host lookup failed")]
    Unavailable,
}

/// Validate the URL and all resolved addresses at registration. Delivery must
/// repeat address classification and connect only to the checked addresses.
/// A lookup that fails, finds no free permit, or exceeds `DNS_TIMEOUT` is
/// `Unavailable`; every other rejection is `NotAllowed`.
// implements: PUSH-256
pub(crate) async fn validate(url: &str, config: &HttpConfig) -> Result<(), WebhookUrlError> {
    validate_with(url, config, &LOOKUPS, DNS_TIMEOUT, |name, port| {
        (name.as_str(), port)
            .to_socket_addrs()
            .map(Iterator::collect)
    })
    .await
}

async fn validate_with<F>(
    url: &str,
    config: &HttpConfig,
    lookups: &'static Semaphore,
    lookup_timeout: std::time::Duration,
    lookup: F,
) -> Result<(), WebhookUrlError>
where
    F: Fn(String, u16) -> std::io::Result<Vec<SocketAddr>> + Send + Sync + 'static,
{
    use WebhookUrlError::*;
    let parsed = url::Url::parse(url).map_err(|_| NotAllowed)?;
    if parsed.scheme() != "https" || !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(NotAllowed);
    }
    let host = parsed.host().ok_or(NotAllowed)?;
    if let Some(domains) = &config.allowed_domains
        && !domains.is_empty()
    {
        let name = host.to_string();
        if !domains.iter().any(|domain| matches_domain(&name, domain)) {
            return Err(NotAllowed);
        }
    }
    let port = parsed.port_or_known_default().ok_or(NotAllowed)?;
    // The host lookup takes its permit first; discovery runs beside it only on
    // a spare permit, which it also keeps until its thread returns.
    let permit = match host {
        url::Host::Domain(_) => Some(lookups.try_acquire().map_err(|_| Unavailable)?),
        _ => None,
    };
    let lookup = Arc::new(lookup);
    let discovery = (!config.allow_private_addresses && !matches!(host, url::Host::Ipv4(_)))
        .then(|| lookups.try_acquire().ok())
        .flatten()
        .map(|permit| {
            let lookup = lookup.clone();
            let task = tokio::task::spawn_blocking(move || {
                let _permit = permit;
                lookup(NAT64_DISCOVERY_NAME.into(), 0)
            });
            async { task.await.map_err(std::io::Error::other)? }
        });
    let nat64 = discover_nat64(discovery);
    let (addresses, nat64): (Vec<SocketAddr>, _) = match host {
        url::Host::Ipv4(ip) => (vec![SocketAddr::new(ip.into(), port)], vec![]),
        url::Host::Ipv6(ip) => (vec![SocketAddr::new(ip.into(), port)], nat64.await),
        url::Host::Domain(name) => {
            let name = name.to_owned();
            let resolve = tokio::task::spawn_blocking(move || {
                // A timed-out lookup keeps its thread, so it keeps its permit.
                let _permit = permit;
                lookup(name, port)
            });
            let resolve = xmtp_common::time::timeout(lookup_timeout, resolve);
            let (Ok(Ok(Ok(addresses))), nat64) = tokio::join!(resolve, nat64) else {
                return Err(Unavailable);
            };
            (addresses, nat64)
        }
    };
    if addresses.is_empty()
        || (!config.allow_private_addresses && addresses.iter().any(|a| blocked(a.ip(), &nat64)))
    {
        return Err(NotAllowed);
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

fn deprecated_site_local(ip: std::net::Ipv6Addr) -> bool {
    (ip.segments()[0] & SITE_LOCAL_MASK) == SITE_LOCAL_NETWORK
}

/// PUSH blocked classes: ATCH private addresses, including those under a
/// discovered NAT64 prefix, plus deprecated site-local.
// implements: PUSH-256
pub(crate) fn blocked(ip: IpAddr, nat64: &[(Ipv6Addr, u8)]) -> bool {
    xmtp_common::address::is_private_behind(ip, nat64)
        || matches!(ip, IpAddr::V6(ip) if deprecated_site_local(ip))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io, sync::mpsc, time::Duration};

    const URL: &str = "https://webhook.example/hook";
    const SHORT_TIMEOUT: Duration = Duration::from_millis(10);

    fn addresses(ips: &[&str]) -> io::Result<Vec<SocketAddr>> {
        Ok(ips
            .iter()
            .map(|ip| SocketAddr::new(ip.parse().unwrap(), 443))
            .collect())
    }

    /// A lookup that returns only once the sender is used or dropped.
    fn blocked_lookup() -> (
        mpsc::Sender<()>,
        impl Fn(String, u16) -> io::Result<Vec<SocketAddr>> + Send + Sync + 'static,
    ) {
        let (release, blocked) = mpsc::channel::<()>();
        let blocked = std::sync::Mutex::new(blocked);
        (release, move |_, _| {
            let blocked = blocked.lock().map_err(|_| io::Error::other("poisoned"))?;
            blocked.recv().map(|()| vec![]).map_err(io::Error::other)
        })
    }

    // verifies: PUSH-256
    #[xmtp_common::test(unwrap_try = true)]
    async fn lookup_failures_are_unavailable_and_answers_are_classified() {
        static PERMITS: Semaphore = Semaphore::const_new(MAX_LOOKUPS);
        let config = HttpConfig::default();
        // The lookup gives the same answer for `ipv4only.arpa`; under the
        // discovered prefix `2a00:1450:4001:64::/96`, `::a00:1` is 10.0.0.1.
        let nat64 = &["2a00:1450:4001:64::c000:aa"][..];
        for (url, answer, expected) in [
            (URL, None, Err(WebhookUrlError::Unavailable)),
            (URL, Some(&[][..]), Err(WebhookUrlError::NotAllowed)),
            (
                URL,
                Some(&["10.0.0.1"][..]),
                Err(WebhookUrlError::NotAllowed),
            ),
            (URL, Some(&["8.8.8.8"][..]), Ok(())),
            (URL, Some(nat64), Err(WebhookUrlError::NotAllowed)),
            (
                "https://[2a00:1450:4001:64::a00:1]/",
                Some(nat64),
                Err(WebhookUrlError::NotAllowed),
            ),
        ] {
            let lookup = move |_, _| answer.map_or(Err(io::Error::other("failed")), addresses);
            assert_eq!(
                validate_with(url, &config, &PERMITS, DNS_TIMEOUT, lookup).await,
                expected,
                "{url} {answer:?}"
            );
        }
        let (_release, lookup) = blocked_lookup();
        let result = validate_with(URL, &config, &PERMITS, SHORT_TIMEOUT, lookup).await;
        assert_eq!(result, Err(WebhookUrlError::Unavailable));
    }

    // verifies: PUSH-256
    #[xmtp_common::test(unwrap_try = true)]
    async fn timed_out_lookup_keeps_its_permit_until_the_thread_returns() {
        static PERMITS: Semaphore = Semaphore::const_new(1);
        let config = HttpConfig::default();
        let (release, lookup) = blocked_lookup();
        let result = validate_with(URL, &config, &PERMITS, SHORT_TIMEOUT, lookup).await;
        assert_eq!(result, Err(WebhookUrlError::Unavailable));
        assert_eq!(PERMITS.available_permits(), 0);
        let public = |_, _| addresses(&["8.8.8.8"]);
        let second = validate_with(URL, &config, &PERMITS, DNS_TIMEOUT, public).await;
        assert_eq!(second, Err(WebhookUrlError::Unavailable));
        release.send(())?;
        let _permit = xmtp_common::time::timeout(DNS_TIMEOUT, PERMITS.acquire()).await??;
    }
}
