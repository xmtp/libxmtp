//! Download address checks.
//!
//! `address-registry.txt` is a snapshot of the IANA IPv4 and IPv6 registries:
//! <https://www.iana.org/assignments/iana-ipv4-special-registry/iana-ipv4-special-registry.xhtml>
//! <https://www.iana.org/assignments/iana-ipv6-special-registry/iana-ipv6-special-registry.xhtml>
//! Both `<updated>` values were 2025-10-09 when checked on 2026-09-25.
//! Refresh the snapshot when either `<updated>` value changes. A longer entry
//! overrides its parent entry.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

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
pub(crate) fn is_private(address: IpAddr) -> bool {
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
    fn parse_row_accepts_tab_separator() {
        assert_eq!(
            parse_row("10.0.0.0/8\tfalse"),
            Some(("10.0.0.0".parse()?, 8, false))
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn parse_row_rejects_malformed() {
        for row in [
            "10.0.0.0/8",
            "10.0.0.0/8 true extra",
            "bad/8 true",
            "10.0.0.0 true",
            "10.0.0.0/33 true",
            "2001:db8::/129 true",
            "10.1.0.0/8 true",
            "2001:db8::1/32 true",
            "10.0.0.0/8 TRUE",
        ] {
            assert_eq!(parse_row(row), None, "{row}");
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
    }
}
