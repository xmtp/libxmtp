use std::net::IpAddr;

const LOCAL_DOMAIN: &str = "xmtpd.local";
const REMOTE_DOMAIN: &str = "sslip.io";

/// Determines how xnet constructs service hostnames.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddressMode {
    /// Local mode: hostnames like `{name}.xmtpd.local`
    Local,
    /// Remote mode: hostnames like `{name}.{ip-dashed}.sslip.io`
    Remote(IpAddr),
}

impl AddressMode {
    /// Build a hostname for the given service name.
    ///
    /// - Local:  `{name}.xmtpd.local`
    /// - Remote: `{name}.{ip}.sslip.io` (dots and colons replaced by dashes)
    pub fn hostname(&self, name: &str) -> String {
        match self {
            Self::Local => format!("{name}.{LOCAL_DOMAIN}"),
            Self::Remote(ip) => {
                let ip_dashed = ip.to_string().replace(['.', ':'], "-");
                format!("{name}.{ip_dashed}.{REMOTE_DOMAIN}")
            }
        }
    }

    /// The DNS domain suffix used in CoreDNS template matching.
    pub fn dns_domain(&self) -> &str {
        match self {
            Self::Local => LOCAL_DOMAIN,
            Self::Remote(_) => REMOTE_DOMAIN,
        }
    }

    pub fn is_remote(&self) -> bool {
        matches!(self, Self::Remote(_))
    }
}

impl Default for AddressMode {
    fn default() -> Self {
        Self::Local
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_hostname() {
        let mode = AddressMode::Local;
        assert_eq!(mode.hostname("node100"), "node100.xmtpd.local");
        assert_eq!(mode.hostname("xnet-200"), "xnet-200.xmtpd.local");
    }

    #[test]
    fn remote_hostname_ipv4() {
        let ip: IpAddr = "203.0.113.42".parse().unwrap();
        let mode = AddressMode::Remote(ip);
        assert_eq!(mode.hostname("node100"), "node100.203-0-113-42.sslip.io");
        assert_eq!(mode.hostname("xnet-200"), "xnet-200.203-0-113-42.sslip.io");
    }

    #[test]
    fn remote_hostname_ipv6() {
        let ip: IpAddr = "2001:db8::1".parse().unwrap();
        let mode = AddressMode::Remote(ip);
        // IpAddr displays 2001:db8::1, colons become dashes
        assert_eq!(mode.hostname("node100"), "node100.2001-db8--1.sslip.io");
    }

    #[test]
    fn dns_domain() {
        assert_eq!(AddressMode::Local.dns_domain(), "xmtpd.local");
        let ip: IpAddr = "10.0.0.1".parse().unwrap();
        assert_eq!(AddressMode::Remote(ip).dns_domain(), "sslip.io");
    }

    #[test]
    fn is_remote() {
        assert!(!AddressMode::Local.is_remote());
        let ip: IpAddr = "10.0.0.1".parse().unwrap();
        assert!(AddressMode::Remote(ip).is_remote());
    }

    #[test]
    fn default_is_local() {
        assert_eq!(AddressMode::default(), AddressMode::Local);
    }
}
