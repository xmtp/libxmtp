use crate::groups::validated_commit::{CommitValidationError, LibXMTPVersion};

#[test]
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

#[test]
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

#[test]
fn test_parse_and_compare_zero_versions() {
    let v0_0_0 = LibXMTPVersion::parse("0.0.0").unwrap();
    let v0_0_1 = LibXMTPVersion::parse("0.0.1").unwrap();
    let v0_1_0 = LibXMTPVersion::parse("0.1.0").unwrap();
    let v1_0_0 = LibXMTPVersion::parse("1.0.0").unwrap();

    assert!(v0_0_0 < v0_0_1);
    assert!(v0_0_1 < v0_1_0);
    assert!(v0_1_0 < v1_0_0);
}

#[test]
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

#[test]
fn test_multi_segment_pre_release_parses() {
    // Multi-segment pre-release identifiers like `1.0.0-alpha.1` are
    // valid semver 2.0; the hand-rolled parser rejected them because
    // it split on `.` first and required exactly three parts.
    assert!(LibXMTPVersion::parse("1.0.0-alpha.1").is_ok());
    assert!(LibXMTPVersion::parse("1.0.0-rc.1.build.42").is_ok());
}

#[test]
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

#[test]
fn test_parse_invalid_format() {
    for bad in ["1.0", "1.0.0.0", "1.x.0", "a.b.c", "1..0", ""] {
        assert!(
            matches!(
                LibXMTPVersion::parse(bad),
                Err(CommitValidationError::InvalidVersionFormat(_)),
            ),
            "expected {bad:?} to fail parsing"
        );
    }
}

/// `PROPOSALS_MIN_PROTOCOL_VERSION` is the default floor written by
/// `enable_proposals` when the caller doesn't override `min_version`.
/// The send-side clamp in `enable_proposals` refuses any
/// `min_version > own pkg_version`, so this constant being ahead of
/// the workspace version would brick every production call to
/// `enable_proposals` that takes the default. Pin the invariant here
/// so CI fails on a one-sided bump.
#[test]
fn proposals_min_protocol_version_does_not_exceed_workspace_version() {
    let default_floor = LibXMTPVersion::parse(xmtp_configuration::PROPOSALS_MIN_PROTOCOL_VERSION)
        .expect("PROPOSALS_MIN_PROTOCOL_VERSION must be valid semver");
    let workspace = LibXMTPVersion::parse(env!("CARGO_PKG_VERSION"))
        .expect("CARGO_PKG_VERSION must be valid semver");
    assert!(
        default_floor <= workspace,
        "PROPOSALS_MIN_PROTOCOL_VERSION ({}) must be <= CARGO_PKG_VERSION ({}); \
         a higher default would trip the enable_proposals clamp",
        xmtp_configuration::PROPOSALS_MIN_PROTOCOL_VERSION,
        env!("CARGO_PKG_VERSION"),
    );
}
