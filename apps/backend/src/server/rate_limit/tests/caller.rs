//! Which caller a request is charged to.
use super::super::caller::caller_key;
use super::*;
use std::net::IpAddr;

fn ip(address: &str) -> IpAddr {
    address.parse().unwrap()
}

fn networks(networks: &[&str]) -> Vec<ipnet::IpNet> {
    networks
        .iter()
        .map(|network| network.parse().unwrap())
        .collect()
}

/// `X-Forwarded-For` headers with these values, in this order.
fn forwarded_for(values: &[&str]) -> http::HeaderMap {
    let mut headers = http::HeaderMap::new();
    for value in values {
        headers.append("x-forwarded-for", value.parse().unwrap());
    }
    headers
}

fn jwt(sub: Option<&str>, iss: Option<&str>) -> AuthContext {
    AuthContext::Jwt {
        sub: sub.map(str::to_owned),
        iss: iss.map(str::to_owned),
        scopes: Default::default(),
    }
}

/// The key of a request without a credential from `peer` with these headers.
fn ip_key(peer: &str, headers: &[&str], trusted: &[&str]) -> CallerKey {
    caller_key(
        None,
        Some(ip(peer)),
        &forwarded_for(headers),
        &networks(trusted),
    )
}

fn bytes(key: &CallerKey) -> Vec<u8> {
    let mut bytes = Vec::new();
    key.to_bytes(&mut bytes);
    bytes
}

// verifies: RATE-001
#[xmtp_common::test(unwrap_try = true)]
fn credentials_name_the_caller_before_the_address() {
    let peer = Some(ip("192.0.2.1"));
    let headers = forwarded_for(&["203.0.113.9"]);
    let trusted = networks(&["192.0.2.0/24"]);
    let key = |auth: Option<&AuthContext>| caller_key(auth, peer, &headers, &trusted);
    assert_eq!(
        key(Some(&AuthContext::ApiKey {
            name: "bridge-bot".into()
        })),
        CallerKey::ApiKey("bridge-bot".into())
    );
    assert_eq!(
        key(Some(&jwt(Some("user"), Some("https://one.example")))),
        CallerKey::Jwt {
            issuer: "https://one.example".into(),
            subject: "user".into(),
        }
    );
    assert_eq!(
        key(Some(&jwt(Some("user"), None))),
        CallerKey::Jwt {
            issuer: String::new(),
            subject: "user".into(),
        }
    );
    // Two issuers can issue the same subject.
    assert_ne!(
        key(Some(&jwt(Some("user"), Some("https://one.example")))),
        key(Some(&jwt(Some("user"), Some("https://two.example"))))
    );
    // A token without `sub`, and a request while auth is off, are charged to
    // the client address.
    let address = CallerKey::from_ip(ip("203.0.113.9"));
    assert_eq!(key(Some(&jwt(None, Some("https://one.example")))), address);
    assert_eq!(key(None), address);
    // Keys of different auth types never share a charge, however alike
    // their values.
    let named_like_an_address = CallerKey::ApiKey("203.0.113.9".into());
    assert_ne!(bytes(&named_like_an_address), bytes(&address));
    assert_ne!(
        bytes(&CallerKey::ApiKey("user".into())),
        bytes(&CallerKey::Jwt {
            issuer: String::new(),
            subject: "user".into(),
        })
    );
}

// verifies: RATE-003
#[xmtp_common::test(unwrap_try = true)]
fn addresses_key_by_ipv4_or_ipv6_slash_64() {
    assert_eq!(
        ip_key("192.0.2.1", &[], &[]),
        CallerKey::Ipv4([192, 0, 2, 1])
    );
    assert_ne!(ip_key("192.0.2.1", &[], &[]), ip_key("192.0.2.2", &[], &[]));
    assert_eq!(
        ip_key("2001:db8:1:2::1", &[], &[]),
        ip_key("2001:db8:1:2:ffff:ffff:ffff:ffff", &[], &[])
    );
    assert_ne!(
        ip_key("2001:db8:1:2::1", &[], &[]),
        ip_key("2001:db8:1:3::1", &[], &[])
    );
    assert_eq!(
        ip_key("::ffff:192.0.2.1", &[], &[]),
        CallerKey::Ipv4([192, 0, 2, 1])
    );
}

// verifies: RATE-002
#[xmtp_common::test(unwrap_try = true)]
fn forwarded_addresses_count_only_behind_trusted_proxies() {
    let trusted = ["10.0.0.0/8"];
    // An untrusted peer's header is whatever the caller wrote.
    assert_eq!(
        ip_key("192.0.2.1", &["203.0.113.9"], &trusted),
        CallerKey::from_ip(ip("192.0.2.1"))
    );
    // A trusted peer without the header is the client.
    assert_eq!(
        ip_key("10.0.0.1", &[], &trusted),
        CallerKey::from_ip(ip("10.0.0.1"))
    );
    // The rightmost entry that is not a trusted proxy, whatever the caller
    // wrote to its left.
    assert_eq!(
        ip_key(
            "10.0.0.1",
            &["198.51.100.66, 203.0.113.9, 10.0.0.2"],
            &trusted
        ),
        CallerKey::from_ip(ip("203.0.113.9"))
    );
    // A list of trusted proxies only selects its leftmost entry.
    assert_eq!(
        ip_key("10.0.0.1", &["10.0.0.3, 10.0.0.2"], &trusted),
        CallerKey::from_ip(ip("10.0.0.3"))
    );
    // An entry that is not an address, met before one is selected, selects
    // the peer; one to the left of the selected entry does not matter.
    for header in [
        "203.0.113.9, unknown, 10.0.0.2",
        "203.0.113.9, 10.0.0.2:443",
        "203.0.113.9,",
    ] {
        assert_eq!(
            ip_key("10.0.0.1", &[header], &trusted),
            CallerKey::from_ip(ip("10.0.0.1")),
            "{header}"
        );
    }
    assert_eq!(
        ip_key("10.0.0.1", &["unknown, 203.0.113.9"], &trusted),
        CallerKey::from_ip(ip("203.0.113.9"))
    );
    // Two headers are one list in header order.
    assert_eq!(
        ip_key(
            "10.0.0.1",
            &["198.51.100.66", "203.0.113.9, 10.0.0.2"],
            &trusted
        ),
        CallerKey::from_ip(ip("203.0.113.9"))
    );
    assert_eq!(
        ip_key("10.0.0.1", &["203.0.113.9", "10.0.0.2"], &trusted),
        CallerKey::from_ip(ip("203.0.113.9"))
    );
    assert_eq!(
        ip_key("10.0.0.1", &["203.0.113.9", "unknown"], &trusted),
        CallerKey::from_ip(ip("10.0.0.1"))
    );
    // A selected IPv6 entry is keyed by its /64.
    assert_eq!(
        ip_key("10.0.0.1", &["2001:db8:1:2::9"], &trusted),
        CallerKey::from_ip(ip("2001:db8:1:2::1"))
    );
}

// verifies: RATE-002
#[xmtp_common::test(unwrap_try = true)]
fn ipv4_mapped_addresses_match_ipv4_proxy_networks() {
    // A dual-stack listener reports an IPv4 proxy in its mapped form.
    assert_eq!(
        ip_key("::ffff:10.0.0.1", &["203.0.113.9"], &["10.0.0.0/8"]),
        CallerKey::from_ip(ip("203.0.113.9"))
    );
    assert_eq!(
        ip_key(
            "10.0.0.1",
            &["203.0.113.9, ::ffff:10.0.0.2"],
            &["10.0.0.0/8"]
        ),
        CallerKey::from_ip(ip("203.0.113.9"))
    );
    // An IPv4-mapped network in the configuration matches the IPv4 peer.
    let trusted = config("trusted_proxies = [\"::ffff:10.0.0.0/104\"]").trusted_proxies;
    assert_eq!(
        caller_key(
            None,
            Some(ip("10.0.0.1")),
            &forwarded_for(&["203.0.113.9"]),
            &trusted
        ),
        CallerKey::from_ip(ip("203.0.113.9"))
    );
}
