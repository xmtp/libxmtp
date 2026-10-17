use thiserror::Error;

/// A version string that is not valid semver.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("Invalid version format: {0}")]
pub struct InvalidVersionFormat(pub String);

/// Wrapper around [`semver::Version`] used for the
/// `MIN_SUPPORTED_PROTOCOL_VERSION` floor and related min-version checks.
///
/// Delegates parsing and ordering to the [`semver`] crate so behavior
/// matches the semver 2.0 spec — most importantly:
///
/// * Pre-release versions sort *before* the release: `1.0.0-alpha <
///   1.0.0-beta < 1.0.0`. The previous hand-rolled implementation got
///   this backwards (`1.0.0 < 1.0.0-alpha`), which would silently
///   pause clients running release builds against any group floor set
///   by a caller passing a pre-release string.
/// * Pre-release identifiers compare numerically when all-digits, so
///   `rc2 < rc10` instead of lexicographic `rc10 < rc2`.
/// * Multi-segment pre-release tags like `1.0.0-alpha.1` parse cleanly
///   instead of failing with `InvalidVersionFormat`.
/// * Build metadata (after `+`) parses cleanly. Note: the [`semver`]
///   crate's `Ord` impl deliberately *includes* build metadata for
///   total-ordering / `Hash` consistency, deviating from semver 2.0
///   §10 ("build metadata MUST be ignored when determining version
///   precedence"). Irrelevant in practice — `CARGO_PKG_VERSION` and
///   the application-facing `update_group_min_version` callers never
///   pass `+`-suffixed input.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct LibXMTPVersion(semver::Version);

impl LibXMTPVersion {
    pub fn parse(version_str: &str) -> Result<Self, InvalidVersionFormat> {
        semver::Version::parse(version_str)
            .map(Self)
            .map_err(|_| InvalidVersionFormat(version_str.to_string()))
    }

    /// The parsed form. Spec 006 compares a published minimum against this.
    pub fn semver(&self) -> &semver::Version {
        &self.0
    }
}

impl std::fmt::Display for LibXMTPVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[xmtp_common::test(unwrap_try = true)]
    fn test_parse_and_compare_basic_versions() {
        let v1_0_0 = LibXMTPVersion::parse("1.0.0").unwrap();
        let v1_0_1 = LibXMTPVersion::parse("1.0.1").unwrap();
        let v1_1_0 = LibXMTPVersion::parse("1.1.0").unwrap();
        let v2_0_0 = LibXMTPVersion::parse("2.0.0").unwrap();

        assert!(v1_0_0 < v1_0_1);
        assert!(v1_0_1 > v1_0_0);

        assert!(v1_0_1 < v1_1_0);
        assert!(v1_1_0 > v1_0_1);

        assert!(v1_1_0 < v2_0_0);
        assert!(v2_0_0 > v1_1_0);

        let v1_0_0_dup = LibXMTPVersion::parse("1.0.0").unwrap();
        assert_eq!(v1_0_0, v1_0_0_dup);
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn test_parse_and_compare_with_suffixes() {
        let v1_0_0 = LibXMTPVersion::parse("1.0.0").unwrap();
        let v1_0_0_alpha = LibXMTPVersion::parse("1.0.0-alpha").unwrap();
        let v1_0_0_beta = LibXMTPVersion::parse("1.0.0-beta").unwrap();
        let v1_0_0_rc1 = LibXMTPVersion::parse("1.0.0-rc1").unwrap();
        let v1_0_1_alpha = LibXMTPVersion::parse("1.0.1-alpha").unwrap();

        // Pre-release identifiers compare alphabetically when non-numeric.
        assert!(v1_0_0_alpha < v1_0_0_beta);
        assert!(v1_0_0_beta < v1_0_0_rc1);

        // Per semver 2.0 §11: a pre-release version sorts BEFORE the
        // corresponding release. This is the correctness fix the
        // semver-crate swap delivers — the old hand-rolled comparison had
        // the relationship inverted.
        assert!(v1_0_0_alpha < v1_0_0);
        assert!(v1_0_0_beta < v1_0_0);

        // Numeric parts take precedence over the pre-release tag.
        assert!(v1_0_0 < v1_0_1_alpha);
        assert!(v1_0_0_rc1 < v1_0_1_alpha);
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn test_parse_and_compare_zero_versions() {
        let v0_0_0 = LibXMTPVersion::parse("0.0.0").unwrap();
        let v0_0_1 = LibXMTPVersion::parse("0.0.1").unwrap();
        let v0_1_0 = LibXMTPVersion::parse("0.1.0").unwrap();
        let v1_0_0 = LibXMTPVersion::parse("1.0.0").unwrap();

        assert!(v0_0_0 < v0_0_1);
        assert!(v0_0_1 < v0_1_0);
        assert!(v0_1_0 < v1_0_0);
    }

    // verifies: GMOD-027
    #[xmtp_common::test(unwrap_try = true)]
    fn test_numeric_pre_release_identifiers_compare_numerically() {
        // Per semver 2.0 §11.4.1: identifiers consisting only of digits
        // are compared numerically. The hand-rolled implementation
        // compared lexicographically, which got `rc10 < rc2` because of
        // ASCII ordering; this test pins the fix.
        let v1_0_0_rc_2 = LibXMTPVersion::parse("1.0.0-rc.2").unwrap();
        let v1_0_0_rc_10 = LibXMTPVersion::parse("1.0.0-rc.10").unwrap();
        assert!(v1_0_0_rc_2 < v1_0_0_rc_10);

        let v1_0_0_alpha_2 = LibXMTPVersion::parse("1.0.0-alpha.2").unwrap();
        let v1_0_0_alpha_11 = LibXMTPVersion::parse("1.0.0-alpha.11").unwrap();
        assert!(v1_0_0_alpha_2 < v1_0_0_alpha_11);
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn test_multi_segment_pre_release_parses() {
        // Multi-segment pre-release identifiers like `1.0.0-alpha.1` are
        // valid semver 2.0; the hand-rolled parser rejected them because
        // it split on `.` first and required exactly three parts.
        assert!(LibXMTPVersion::parse("1.0.0-alpha.1").is_ok());
        assert!(LibXMTPVersion::parse("1.0.0-rc.1.build.42").is_ok());
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn test_build_metadata_parses() {
        // Build metadata strings (`+...`) are accepted by the parser. Note
        // that the [`semver`] crate's `Ord` impl deliberately includes
        // build metadata for total-ordering / `Hash` consistency, which
        // deviates from semver 2.0 §10 ("build metadata MUST be ignored
        // when determining version precedence"). This is irrelevant for
        // libxmtp's floor comparison because no caller passes a `+`-
        // suffixed string today — `CARGO_PKG_VERSION` is a plain
        // `X.Y.Z`, and the application-facing `update_group_min_version`
        // host API never injects build metadata. Pinning parse-success
        // here so a future caller that *does* pass `+`-suffixed input
        // gets predictable behavior instead of `InvalidVersionFormat`.
        assert!(LibXMTPVersion::parse("1.0.0+build.5").is_ok());
        assert!(LibXMTPVersion::parse("1.0.0-rc.1+build.5").is_ok());
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn test_parse_invalid_format() {
        for bad in ["1.0", "1.0.0.0", "1.x.0", "a.b.c", "1..0", ""] {
            assert_eq!(
                LibXMTPVersion::parse(bad),
                Err(InvalidVersionFormat(bad.to_string())),
                "expected {bad:?} to fail parsing"
            );
        }
    }

    /// Creation must not write a floor above the version of the creating client.
    #[xmtp_common::test(unwrap_try = true)]
    fn proposals_min_protocol_version_does_not_exceed_workspace_version() {
        let default_floor =
            LibXMTPVersion::parse(xmtp_configuration::PROPOSALS_MIN_PROTOCOL_VERSION)
                .expect("PROPOSALS_MIN_PROTOCOL_VERSION must be valid semver");
        let workspace = LibXMTPVersion::parse(env!("CARGO_PKG_VERSION"))
            .expect("CARGO_PKG_VERSION must be valid semver");
        assert!(
            default_floor <= workspace,
            "PROPOSALS_MIN_PROTOCOL_VERSION ({}) must be <= CARGO_PKG_VERSION ({}); \
             a higher default would pause freshly created groups",
            xmtp_configuration::PROPOSALS_MIN_PROTOCOL_VERSION,
            env!("CARGO_PKG_VERSION"),
        );
    }
}
