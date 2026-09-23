//! The committed `MIN_SUPPORTED_PROTOCOL_VERSION` floor check.

use openmls::group::MlsGroup as OpenMlsGroup;

use super::component_id::ComponentId;
use crate::libxmtp_version::LibXMTPVersion;

/// Returns the group's committed `MIN_SUPPORTED_PROTOCOL_VERSION` floor
/// when it exceeds `own_version`, reading ONLY the pre-commit AppData
/// dict — committed, already-validated state.
///
/// This is the shared trigger for the "pause, don't fork" guards on the
/// `xmtp_mls` receive paths (`process_message_with_app_data` before
/// dispatch; `ValidatedCommit::from_staged_commit` before interpreting
/// migrated group state). It is deliberately blind to any floor bump
/// carried by the commit currently being processed: that proposal has not passed
/// the super-admin policy check yet, and a pause triggered by
/// unvalidated input would let any member freeze the group permanently.
/// The commit that *raises* the floor pauses below-floor receivers
/// through the post-policy check at the end of commit validation
/// instead. Consequence for protocol evolution: a release introducing
/// a new wire format must land the group-floor bump in a *strictly
/// earlier* commit than the first commit using that format.
///
/// Lenient on malformed state (non-UTF-8 floor bytes, unparseable
/// semver ⇒ `None`), mirroring `enforce_min_version_monotonicity`'s
/// treatment of malformed priors: garbage must never brick the group.
pub fn committed_floor_exceeding(mls_group: &OpenMlsGroup, own: &LibXMTPVersion) -> Option<String> {
    committed_floor_exceeding_in_extensions(mls_group.extensions(), own)
}

/// Extensions-only variant of [`committed_floor_exceeding`], split out so
/// unit tests can exercise the parse-and-compare logic without
/// materializing an `OpenMlsGroup`.
pub fn committed_floor_exceeding_in_extensions(
    extensions: &openmls::extensions::Extensions<openmls::group::GroupContext>,
    own: &LibXMTPVersion,
) -> Option<String> {
    let bytes = extensions
        .app_data_dictionary()?
        .dictionary()
        .get(&ComponentId::MIN_SUPPORTED_PROTOCOL_VERSION.as_u16())?
        .to_vec();
    let floor = String::from_utf8(bytes).ok()?;
    let floor_version = LibXMTPVersion::parse(&floor).ok()?;
    (floor_version > *own).then_some(floor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openmls::extensions::{
        AppDataDictionary, AppDataDictionaryExtension, Extension, Extensions,
    };

    fn extensions_with_dict(
        entries: &[(u16, Vec<u8>)],
    ) -> Extensions<openmls::group::GroupContext> {
        let mut dict = AppDataDictionary::new();
        for (id, bytes) in entries {
            let _ = dict.insert(*id, bytes.clone());
        }
        Extensions::from_vec(vec![Extension::AppDataDictionary(
            AppDataDictionaryExtension::new(dict),
        )])
        .expect("AppDataDictionary is a valid GroupContext extension")
    }

    fn empty_extensions() -> Extensions<openmls::group::GroupContext> {
        Extensions::from_vec(vec![]).expect("empty extensions are always valid")
    }

    /// Parse a semver string the way the production caller does (once, from
    /// the client's own `pkg_version`). Panics on invalid input — matching
    /// `VersionInfo`, which asserts its own version is valid at construction.
    fn ver(s: &str) -> LibXMTPVersion {
        LibXMTPVersion::parse(s).unwrap()
    }

    // ========================================================================
    // committed_floor_exceeding_in_extensions
    // ========================================================================
    //
    // The shared trigger for the pause-before-parse guards. Two properties
    // are load-bearing: (1) it fires strictly on floor > own — equal or
    // lower floors must not pause; (2) it is lenient on garbage — malformed
    // floor bytes must read as "no floor", never as an error that could
    // wedge the group.

    #[xmtp_common::test(unwrap_try = true)]
    fn floor_above_own_version_fires() {
        let exts = extensions_with_dict(&[(
            ComponentId::MIN_SUPPORTED_PROTOCOL_VERSION.as_u16(),
            b"2.0.0".to_vec(),
        )]);
        assert_eq!(
            committed_floor_exceeding_in_extensions(&exts, &ver("1.11.0")),
            Some("2.0.0".to_string())
        );
        // Prerelease floors order correctly under semver: 1.11.0-dev
        // exceeds 1.10.0 but not 1.11.0.
        let exts = extensions_with_dict(&[(
            ComponentId::MIN_SUPPORTED_PROTOCOL_VERSION.as_u16(),
            b"1.11.0-dev".to_vec(),
        )]);
        assert_eq!(
            committed_floor_exceeding_in_extensions(&exts, &ver("1.10.0")),
            Some("1.11.0-dev".to_string())
        );
        assert_eq!(
            committed_floor_exceeding_in_extensions(&exts, &ver("1.11.0")),
            None
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn floor_at_or_below_own_version_does_not_fire() {
        let exts = extensions_with_dict(&[(
            ComponentId::MIN_SUPPORTED_PROTOCOL_VERSION.as_u16(),
            b"1.11.0".to_vec(),
        )]);
        // Equal: not paused — the floor is inclusive.
        assert_eq!(
            committed_floor_exceeding_in_extensions(&exts, &ver("1.11.0")),
            None
        );
        // Above: not paused.
        assert_eq!(
            committed_floor_exceeding_in_extensions(&exts, &ver("1.12.0")),
            None
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn missing_floor_or_dict_does_not_fire() {
        assert_eq!(
            committed_floor_exceeding_in_extensions(&empty_extensions(), &ver("1.11.0")),
            None
        );
        assert_eq!(
            committed_floor_exceeding_in_extensions(&extensions_with_dict(&[]), &ver("1.11.0")),
            None
        );
        // Dict present with other components but no floor entry.
        let exts = extensions_with_dict(&[(ComponentId::GROUP_NAME.as_u16(), b"name".to_vec())]);
        assert_eq!(
            committed_floor_exceeding_in_extensions(&exts, &ver("1.11.0")),
            None
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn malformed_floor_is_lenient() {
        // Non-UTF-8 bytes → no floor, never an error.
        let exts = extensions_with_dict(&[(
            ComponentId::MIN_SUPPORTED_PROTOCOL_VERSION.as_u16(),
            vec![0xFF, 0xFE],
        )]);
        assert_eq!(
            committed_floor_exceeding_in_extensions(&exts, &ver("1.11.0")),
            None
        );
        // Unparseable floor semver → no floor.
        let exts = extensions_with_dict(&[(
            ComponentId::MIN_SUPPORTED_PROTOCOL_VERSION.as_u16(),
            b"not-a-version".to_vec(),
        )]);
        assert_eq!(
            committed_floor_exceeding_in_extensions(&exts, &ver("1.11.0")),
            None
        );
        // The client's own version can no longer be unparseable here: it is
        // parsed once and asserted valid when `VersionInfo` is built, so this
        // guard only ever compares against a valid `LibXMTPVersion`.
    }
}
