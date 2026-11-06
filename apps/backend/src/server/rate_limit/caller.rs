//! The caller a request is charged to.

use crate::{auth::AuthContext, config::CallerKey};
use http::HeaderMap;
use ipnet::IpNet;
use std::net::{IpAddr, Ipv4Addr};

const X_FORWARDED_FOR: &str = "x-forwarded-for";

/// The caller key of a request: the API key's name, a JWT's issuer and
/// subject, or else the client address. `auth` is the context auth admitted
/// the request under, absent when auth is off; `peer` is the connection's
/// remote address. A JWT without `sub` names no caller, so it is charged to
/// its address like a request without a credential.
// implements: RATE-001
pub(super) fn caller_key(
    auth: Option<&AuthContext>,
    peer: Option<IpAddr>,
    headers: &HeaderMap,
    trusted_proxies: &[IpNet],
) -> CallerKey {
    match auth {
        Some(AuthContext::ApiKey { name }) => CallerKey::ApiKey(name.clone()),
        Some(AuthContext::Jwt {
            sub: Some(subject),
            iss,
            ..
        }) => CallerKey::Jwt {
            issuer: iss.clone().unwrap_or_default(),
            subject: subject.clone(),
        },
        _ => {
            // Every connection tonic accepts has a peer address. Should one
            // ever lack it, its requests share one allowance rather than go
            // uncharged.
            let peer = peer.unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED));
            CallerKey::from_ip(client_address(peer, headers, trusted_proxies))
        }
    }
}

/// The client address: the peer, unless the peer is a trusted proxy, in which
/// case the first `X-Forwarded-For` entry from the right that is an address
/// and not a trusted proxy. Every header is one list in header order. An
/// entry that is not an address, met before one is selected, selects the
/// peer, because entries further left were written past it; a list of only
/// trusted proxies selects its leftmost entry.
// implements: RATE-002
pub(super) fn client_address(
    peer: IpAddr,
    headers: &HeaderMap,
    trusted_proxies: &[IpNet],
) -> IpAddr {
    // The configuration holds an IPv4-mapped network as the IPv4 network it
    // maps, so a mapped address is compared as its IPv4 form.
    let trusted = |address: IpAddr| {
        let address = address.to_canonical();
        trusted_proxies
            .iter()
            .any(|network| network.contains(&address))
    };
    if !trusted(peer) {
        return peer;
    }
    let mut leftmost = None;
    let entries = headers
        .get_all(X_FORWARDED_FOR)
        .iter()
        .rev()
        .flat_map(|value| value.as_bytes().rsplit(|&byte| byte == b','));
    for entry in entries {
        let Some(address) = std::str::from_utf8(entry.trim_ascii())
            .ok()
            .and_then(|entry| entry.parse::<IpAddr>().ok())
        else {
            return peer;
        };
        if !trusted(address) {
            return address;
        }
        leftmost = Some(address);
    }
    leftmost.unwrap_or(peer)
}
