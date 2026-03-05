//! XMTP network environment configuration.

use super::{GrpcUrlsDev, GrpcUrlsLocal, GrpcUrlsProduction};

/// Represents the XMTP network environment a client connects to.
///
/// There are two categories of environments:
/// - **Centralized (V3):** `Local`, `Dev`, `Production` -- backed by node-go with gRPC.
/// - **Decentralized (D14n):** `TestnetStaging`, `TestnetDev`, `Testnet`, `Mainnet` -- backed by
///   the decentralized XMTP network (xmtpd).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum XmtpEnv {
    // Centralized (V3) environments
    /// Local development environment (node-go running locally).
    Local,
    /// Shared dev environment (node-go hosted by XMTP).
    Dev,
    /// Production environment (node-go hosted by XMTP).
    Production,

    // Decentralized (D14n) environments
    /// Testnet staging (internal pre-release).
    TestnetStaging,
    /// Testnet dev (development iteration).
    TestnetDev,
    /// Public testnet.
    Testnet,
    /// Mainnet (production decentralized network).
    Mainnet,
}

impl XmtpEnv {
    /// Returns the default gRPC API URL for centralized (V3) environments.
    ///
    /// For decentralized (D14n) environments this returns `None`, since they use
    /// xmtpd/gateway endpoints instead of a single node URL.
    pub fn default_api_url(&self) -> Option<&'static str> {
        match self {
            Self::Local => Some(GrpcUrlsLocal::NODE),
            Self::Dev => Some(GrpcUrlsDev::NODE),
            Self::Production => Some(GrpcUrlsProduction::NODE),
            Self::TestnetStaging | Self::TestnetDev | Self::Testnet | Self::Mainnet => None,
        }
    }

    /// Returns `true` if this is a decentralized (D14n) environment.
    pub fn is_d14n(&self) -> bool {
        matches!(
            self,
            Self::TestnetStaging | Self::TestnetDev | Self::Testnet | Self::Mainnet
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centralized_envs_have_api_url() {
        assert!(XmtpEnv::Local.default_api_url().is_some());
        assert!(XmtpEnv::Dev.default_api_url().is_some());
        assert!(XmtpEnv::Production.default_api_url().is_some());
    }

    #[test]
    fn d14n_envs_have_no_api_url() {
        assert!(XmtpEnv::TestnetStaging.default_api_url().is_none());
        assert!(XmtpEnv::TestnetDev.default_api_url().is_none());
        assert!(XmtpEnv::Testnet.default_api_url().is_none());
        assert!(XmtpEnv::Mainnet.default_api_url().is_none());
    }

    #[test]
    fn is_d14n_returns_correct_values() {
        // Centralized envs are NOT d14n
        assert!(!XmtpEnv::Local.is_d14n());
        assert!(!XmtpEnv::Dev.is_d14n());
        assert!(!XmtpEnv::Production.is_d14n());

        // D14n envs ARE d14n
        assert!(XmtpEnv::TestnetStaging.is_d14n());
        assert!(XmtpEnv::TestnetDev.is_d14n());
        assert!(XmtpEnv::Testnet.is_d14n());
        assert!(XmtpEnv::Mainnet.is_d14n());
    }
}
