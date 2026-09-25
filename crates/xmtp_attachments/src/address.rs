//! Download address checks.
//!
//! `address-registry.txt` is the IANA special-purpose registry snapshot from
//! 2025-10-09. A longer entry overrides its parent entry.

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
        if line.starts_with('#') {
            continue;
        }
        let Some((block, value)) = line.split_once(' ') else {
            continue;
        };
        let Some((prefix, bits)) = block.split_once('/') else {
            continue;
        };
        let (Ok(prefix), Ok(bits)) = (prefix.parse(), bits.parse::<u32>()) else {
            continue;
        };
        if bits >= longest && prefix_matches(address, prefix, bits) {
            longest = bits;
            reachable = value == "true";
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
