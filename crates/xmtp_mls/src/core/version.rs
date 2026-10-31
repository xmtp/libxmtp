use std::sync::Arc;

use xmtp_mls_common::libxmtp_version::LibXMTPVersion;

#[derive(Clone, Debug, PartialEq)]
pub struct VersionInfo {
    pkg_version: Arc<str>,
    /// `pkg_version` parsed once at construction. This is the client's own
    /// build identity, not remote data, so it is always valid semver
    /// (see [`VersionInfo::new`]) — receive-path guards can compare against
    /// it without re-parsing the string on every message.
    pkg_semver: LibXMTPVersion,
}

impl Default for VersionInfo {
    fn default() -> Self {
        Self::new(env!("CARGO_PKG_VERSION"))
    }
}

impl VersionInfo {
    /// Build a `VersionInfo`, parsing and caching the semver form.
    ///
    /// `version` is the client's own package version — in production always
    /// the compile-time `CARGO_PKG_VERSION`. A non-semver value is a build
    /// or configuration bug, not runtime data, so it panics here rather than
    /// silently disabling the below-floor pause on every group later.
    fn new(version: &str) -> Self {
        let pkg_semver = LibXMTPVersion::parse(version)
            .unwrap_or_else(|_| panic!("client pkg_version {version:?} is not valid semver"));
        Self {
            pkg_version: version.into(),
            pkg_semver,
        }
    }

    pub fn pkg_version(&self) -> &str {
        &self.pkg_version
    }

    /// The client's own version, parsed once at construction. Prefer this
    /// over re-parsing [`pkg_version`](Self::pkg_version) on hot paths.
    pub fn pkg_semver(&self) -> &LibXMTPVersion {
        &self.pkg_semver
    }

    // Test only function to update the version of the client
    #[cfg(test)]
    pub fn test_update_version(&mut self, version: &str) {
        *self = Self::new(version);
    }
}
