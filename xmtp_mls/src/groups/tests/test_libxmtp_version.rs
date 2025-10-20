use crate::groups::validated_commit::{CommitValidationError, LibXMTPVersion};

#[test]
fn test_parse_and_compare_basic_versions() {
    let v1_0_0 = LibXMTPVersion::parse("1.0.0").unwrap();
    let v1_0_1 = LibXMTPVersion::parse("1.0.1").unwrap();
    let v1_1_0 = LibXMTPVersion::parse("1.1.0").unwrap();
    let v2_0_0 = LibXMTPVersion::parse("2.0.0").unwrap();

    // Test patch version comparison
    assert!(v1_0_0 < v1_0_1);
    assert!(v1_0_1 > v1_0_0);

    // Test minor version comparison
    assert!(v1_0_1 < v1_1_0);
    assert!(v1_1_0 > v1_0_1);

    // Test major version comparison
    assert!(v1_1_0 < v2_0_0);
    assert!(v2_0_0 > v1_1_0);

    // Test equality
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

    // Versions with suffixes compare lexicographically by suffix after version parts
    assert!(v1_0_0_alpha < v1_0_0_beta);
    assert!(v1_0_0_beta < v1_0_0_rc1);

    // Version without suffix (None) is less than version with suffix (Some)
    // because None < Some in Rust's default Ord implementation
    // NOTE: this does not match semver comparison
    assert!(v1_0_0 < v1_0_0_alpha);
    assert!(v1_0_0 < v1_0_0_beta);

    // Numeric parts take precedence over suffix
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
fn test_parse_and_compare_complex_suffixes() {
    let v1_2_3_alpha1 = LibXMTPVersion::parse("1.2.3-alpha1").unwrap();
    let v1_2_3_alpha2 = LibXMTPVersion::parse("1.2.3-alpha2").unwrap();
    let v1_2_3_beta = LibXMTPVersion::parse("1.2.3-beta").unwrap();
    let v1_2_3_dev = LibXMTPVersion::parse("1.2.3-dev").unwrap();
    let v1_2_3_snapshot = LibXMTPVersion::parse("1.2.3-snapshot").unwrap();

    // Lexicographic ordering of suffixes
    assert!(v1_2_3_alpha1 < v1_2_3_alpha2);
    assert!(v1_2_3_alpha2 < v1_2_3_beta);
    assert!(v1_2_3_beta < v1_2_3_dev);
    assert!(v1_2_3_dev < v1_2_3_snapshot);
}

#[test]
fn test_parse_invalid_format() {
    let result = LibXMTPVersion::parse("1.0");
    assert!(matches!(
        result,
        Err(CommitValidationError::InvalidVersionFormat(_))
    ));
    let result = LibXMTPVersion::parse("1.0.0.0");
    assert!(matches!(
        result,
        Err(CommitValidationError::InvalidVersionFormat(_))
    ));
    let result = LibXMTPVersion::parse("1.x.0");
    assert!(matches!(
        result,
        Err(CommitValidationError::InvalidVersionFormat(_))
    ));

    let result = LibXMTPVersion::parse("a.b.c");
    assert!(matches!(
        result,
        Err(CommitValidationError::InvalidVersionFormat(_))
    ));
    let result = LibXMTPVersion::parse("1..0");
    assert!(matches!(
        result,
        Err(CommitValidationError::InvalidVersionFormat(_))
    ));
    let result = LibXMTPVersion::parse("");
    assert!(matches!(
        result,
        Err(CommitValidationError::InvalidVersionFormat(_))
    ));
}

#[test]
fn test_parse_suffix_only() {
    let result = LibXMTPVersion::parse("1.0.0-");
    assert!(result.is_ok());
}
