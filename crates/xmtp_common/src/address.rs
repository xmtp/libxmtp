//! Private address checks for attachment downloads and webhook hosts.
//!
//! `address-registry.txt` is a snapshot of the IANA IPv4 and IPv6 registries:
//! <https://www.iana.org/assignments/iana-ipv4-special-registry/iana-ipv4-special-registry.xhtml>
//! <https://www.iana.org/assignments/iana-ipv6-special-registry/iana-ipv6-special-registry.xhtml>
//! Both `<updated>` values were 2025-10-09 when checked on 2026-09-25.
//! Refresh the snapshot when either `<updated>` value changes. A longer entry
//! overrides its parent entry.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

/// RFC 7050 §2.2: a DNS64 synthesizes this name's AAAA under its NAT64 prefix.
pub const NAT64_DISCOVERY_NAME: &str = "ipv4only.arpa";

fn prefix_matches(address: IpAddr, prefix: IpAddr, bits: u32) -> bool {
    match (address, prefix) {
        (IpAddr::V4(address), IpAddr::V4(prefix)) => {
            let mask = u32::MAX.checked_shl(32 - bits).unwrap_or(0);
            u32::from(address) & mask == u32::from(prefix) & mask
        }
        (IpAddr::V6(address), IpAddr::V6(prefix)) => {
            let mask = u128::MAX.checked_shl(128 - bits).unwrap_or(0);
            u128::from(address) & mask == u128::from(prefix) & mask
        }
        _ => false,
    }
}

fn parse_row(line: &str) -> Option<(IpAddr, u32, bool)> {
    let mut fields = line.split_whitespace();
    let block = fields.next()?;
    let value = fields.next()?;
    if fields.next().is_some() {
        return None;
    }
    let (prefix, bits) = block.split_once('/')?;
    let prefix: IpAddr = prefix.parse().ok()?;
    let bits: u32 = bits.parse().ok()?;
    let aligned = match prefix {
        IpAddr::V4(ip) if bits <= 32 => {
            let mask = u32::MAX.checked_shl(32 - bits).unwrap_or(0);
            u32::from(ip) & mask == u32::from(ip)
        }
        IpAddr::V6(ip) if bits <= 128 => {
            let mask = u128::MAX.checked_shl(128 - bits).unwrap_or(0);
            u128::from(ip) & mask == u128::from(ip)
        }
        _ => return None,
    };
    let reachable = match value {
        "true" => true,
        "false" => false,
        _ => return None,
    };
    aligned.then_some((prefix, bits, reachable))
}

/// Return true for an address that must not be reached by a default client.
pub fn is_private(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(ip) if ip.is_multicast() => return true,
        IpAddr::V6(ip) if ip.is_multicast() => return true,
        IpAddr::V6(ip) if embedded_ipv4(ip).is_some_and(|v4| is_private(IpAddr::V4(v4))) => {
            return true;
        }
        _ => {}
    }

    let mut longest = 0;
    let mut reachable = true;
    for line in include_str!("address-registry.txt").lines() {
        let Some((prefix, bits, row_reachable)) = parse_row(line) else {
            continue;
        };
        if bits >= longest && prefix_matches(address, prefix, bits) {
            longest = bits;
            reachable = row_reachable;
        }
    }
    !reachable
}

/// RFC 6052 §2.2: the IPv4 bytes follow the prefix and skip byte 8.
fn embedded_at(ip: Ipv6Addr, bits: u8) -> Ipv4Addr {
    let octets = ip.octets();
    let mut v4 = (usize::from(bits / 8)..16)
        .filter(|&i| i != 8)
        .map(|i| octets[i]);
    Ipv4Addr::from(std::array::from_fn(|_| v4.next().unwrap_or(0)))
}

/// RFC 7050 §3: the NAT64 prefixes in the answer for `NAT64_DISCOVERY_NAME`.
fn nat64_prefixes(answer: impl IntoIterator<Item = IpAddr>) -> Vec<(Ipv6Addr, u8)> {
    let mut prefixes = vec![];
    for ip in answer {
        let IpAddr::V6(ip) = ip else { continue };
        for bits in [96, 64, 56, 48, 40, 32] {
            if matches!(embedded_at(ip, bits).octets(), [192, 0, 0, 170 | 171]) {
                let mask = u128::MAX << (128 - u32::from(bits));
                prefixes.push((Ipv6Addr::from(u128::from(ip) & mask), bits));
            }
        }
    }
    prefixes
}

/// The prefixes in a `NAT64_DISCOVERY_NAME` answer. No lookup, a failed one,
/// or one slower than 5 seconds adds no prefix.
pub async fn discover_nat64<A, E>(
    lookup: Option<impl Future<Output = Result<A, E>>>,
) -> Vec<(Ipv6Addr, u8)>
where
    A: IntoIterator<Item = SocketAddr>,
{
    let Some(lookup) = lookup else {
        return vec![];
    };
    match crate::time::timeout(std::time::Duration::from_secs(5), lookup).await {
        Ok(Ok(answer)) => nat64_prefixes(answer.into_iter().map(|address| address.ip())),
        _ => vec![],
    }
}

/// `is_private`, or under a discovered NAT64 prefix with a private IPv4 address.
pub fn is_private_behind(address: IpAddr, prefixes: &[(Ipv6Addr, u8)]) -> bool {
    is_private(address)
        || matches!(address, IpAddr::V6(ip) if prefixes.iter().any(|&(prefix, bits)| {
            prefix_matches(address, prefix.into(), bits.into())
                && is_private(embedded_at(ip, bits).into())
        }))
}

fn embedded_ipv4(ip: Ipv6Addr) -> Option<Ipv4Addr> {
    if let Some(v4) = ip.to_ipv4_mapped() {
        return Some(v4);
    }
    let value = u128::from(ip);
    let well_known = u128::from("64:ff9b::".parse::<Ipv6Addr>().unwrap());
    let local_use = u128::from("64:ff9b:1::".parse::<Ipv6Addr>().unwrap());
    let in_well_known = value >> 32 == well_known >> 32;
    let in_local_use = value >> 80 == local_use >> 80;
    (in_well_known || in_local_use).then(|| Ipv4Addr::from(value as u32))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[xmtp_common::test(unwrap_try = true)]
    fn registry_rows_are_well_formed() {
        for (index, line) in include_str!("address-registry.txt").lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            assert!(parse_row(line).is_some(), "line {}: {line}", index + 1);
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn parse_row_accepts_only_well_formed_rows() {
        for (row, expected) in [
            ("10.0.0.0/8\tfalse", Some(("10.0.0.0".parse()?, 8, false))),
            ("10.0.0.0/8", None),
            ("10.0.0.0/8 true extra", None),
            ("bad/8 true", None),
            ("10.0.0.0 true", None),
            ("10.0.0.0/33 true", None),
            ("2001:db8::/129 true", None),
            ("10.1.0.0/8 true", None),
            ("2001:db8::1/32 true", None),
            ("10.0.0.0/8 TRUE", None),
        ] {
            assert_eq!(parse_row(row), expected, "{row}");
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn registry_and_embedded_addresses() {
        for address in [
            "10.0.0.1",
            "::ffff:10.0.0.1",
            "64:ff9b::a00:1",
            "64:ff9b:1::a00:1",
            "::ffff:8.8.8.8",
            "64:ff9b:1::808:808",
            "ff02::1",
            "224.0.0.1",
            "192.0.2.1",
        ] {
            assert!(is_private(address.parse()?), "{address}");
        }
        for address in [
            "8.8.8.8",
            "2606:4700:4700::1111",
            "192.0.0.9",
            "64:ff9b::808:808",
        ] {
            assert!(!is_private(address.parse()?), "{address}");
        }
        let net: Ipv6Addr = "2a00:1450:4001:64::".parse()?;
        let (net96, net64) = ((net, 96), (net, 64));
        for (answer, expected) in [
            ("2a00:1450:4001:64::c000:aa", vec![net96]),
            ("2a00:1450:4001:64:c0:0:aa00:0", vec![net64]),
            ("2001:db8:c000:ab::", vec![("2001:db8::".parse()?, 32)]),
            ("192.0.0.170", vec![]),
        ] {
            assert_eq!(nat64_prefixes([answer.parse()?]), expected, "{answer}");
        }
        for (address, prefixes, expected) in [
            ("2a00:1450:4001:64:a:0:100:0", net64, true),
            ("2a00:1450:4001:64::808:808", net96, false),
            ("2606:4700:4700::1111", net96, false),
        ] {
            assert_eq!(
                is_private_behind(address.parse()?, &[prefixes]),
                expected,
                "{address}"
            );
        }
    }
}
