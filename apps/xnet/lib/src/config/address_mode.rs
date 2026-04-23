const LOCAL_DOMAIN: &str = "xmtpd.local";

/// Determines how xnet constructs service hostnames.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddressMode {
    /// Local mode: hostnames like `{name}.xmtpd.local`
    Local,
    /// Remote domain mode: hostnames like `{name}.{domain}`
    RemoteDomain(String),
}

impl AddressMode {
    /// Build a hostname for the given service name.
    ///
    /// - Local:        `{name}.xmtpd.local`
    /// - RemoteDomain: `{name}.{domain}`
    pub fn hostname(&self, name: &str) -> String {
        match self {
            Self::Local => format!("{name}.{LOCAL_DOMAIN}"),
            Self::RemoteDomain(domain) => format!("{name}.{domain}"),
        }
    }

    /// The DNS domain suffix used in CoreDNS template matching.
    pub fn dns_domain(&self) -> &str {
        match self {
            Self::Local => LOCAL_DOMAIN,
            Self::RemoteDomain(domain) => domain,
        }
    }

    pub fn is_remote(&self) -> bool {
        matches!(self, Self::RemoteDomain(_))
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
    fn dns_domain() {
        assert_eq!(AddressMode::Local.dns_domain(), "xmtpd.local");
        assert_eq!(
            AddressMode::RemoteDomain("xmtp.run".to_string()).dns_domain(),
            "xmtp.run"
        );
    }

    #[test]
    fn is_remote() {
        assert!(!AddressMode::Local.is_remote());
        assert!(AddressMode::RemoteDomain("xmtp.run".to_string()).is_remote());
    }

    #[test]
    fn remote_domain_hostname() {
        let mode = AddressMode::RemoteDomain("xmtp.run".to_string());
        assert_eq!(mode.hostname("node100"), "node100.xmtp.run");
        assert_eq!(mode.hostname("xnet-200"), "xnet-200.xmtp.run");
    }
}
