use super::*;
use crate::ValidatedConfig;
use xmtp_configuration::ApplicationComponentError;

const MINIMAL: &str =
    "[database]\nurl = 'postgres://localhost/xmtp'\n[server]\nidentifier = 'org.xmtp.test'\n";

/// One `[[application_components]]` table with every key set.
fn entry(component_id: u32, name: &str) -> String {
    format!(
        "\n[[application_components]]\ncomponent_id = {component_id}\nname = '{name}'\n\
         component_type = 'tls_map_inbox_id_string'\ninsert_policy = 'allow'\n\
         update_policy = 'deny'\ndelete_policy = 'allow_if_admin'\n\
         in_groups = true\nin_dms = false\n"
    )
}

fn load(entries: &[String]) -> Result<ValidatedConfig, ConfigError> {
    ValidatedConfig::load_str(&format!("{MINIMAL}{}", entries.concat()))
}

/// A definition no client could register, or one that makes an ID or name
/// ambiguous, stops startup. The error names the key by the entry's position
/// and never the entry's contents, which may have come from the environment.
// verifies: CONF-078
#[xmtp_common::test(unwrap_try = true)]
fn an_invalid_entry_stops_startup_at_its_position() {
    load(&[entry(0xC000, "a"), entry(0xFEFF, &"b".repeat(100))])?;

    let refused = |entries: &[String], field: &str, reason| {
        let error = load(entries).unwrap_err();
        assert!(
            matches!(&error, ConfigError::ApplicationComponent { reason: r, .. } if *r == reason),
            "expected {reason:?} at {field}, got {error}"
        );
        assert_eq!(error.field(), Some(field), "{error}");
        assert!(!error.to_string().contains("SECRET"), "{error}");
    };
    use ApplicationComponentError as E;
    refused(
        &[entry(0xBFFF, "SECRET")],
        "application_components[0].component_id",
        E::ComponentId,
    );
    refused(
        &[entry(0xFF00, "SECRET")],
        "application_components[0].component_id",
        E::ComponentId,
    );
    refused(
        &[entry(0xC000, "")],
        "application_components[0].name",
        E::Name,
    );
    refused(
        &[entry(0xC000, &"SECRET".repeat(17))],
        "application_components[0].name",
        E::Name,
    );
    refused(
        &[entry(0xC000, "SECRET").replace("in_groups = true", "in_groups = false")],
        "application_components[0].in_groups",
        E::Conversations,
    );
    refused(
        &[
            entry(0xC000, "a"),
            entry(0xC001, "b"),
            entry(0xC000, "SECRET"),
        ],
        "application_components[2].component_id",
        E::DuplicateId,
    );
    refused(
        &[entry(0xC000, "SECRET"), entry(0xC001, "SECRET")],
        "application_components[1].name",
        E::DuplicateName,
    );

    let error = load(&[entry(0xC000, "a"), entry(0xC001, "USER_DISPLAY_NAME")]).unwrap_err();
    assert!(
        matches!(error, ConfigError::WellKnownComponentName { .. }),
        "{error}"
    );
    assert_eq!(
        error.field(),
        Some("application_components[1].name"),
        "{error}"
    );
    // The clash is with the exact META name only.
    load(&[entry(0xC000, "user_display_name")])?;
}

/// A type or policy outside the supported set, a missing policy, and an ID
/// wider than 16 bits are not a definition at all, so the file does not
/// decode. The refusal names the key.
// verifies: CONF-078
#[xmtp_common::test(unwrap_try = true)]
fn an_unsupported_type_or_missing_policy_does_not_decode() {
    for (source, key) in [
        (
            entry(0xC000, "a").replace("tls_map_inbox_id_string", "unspecified"),
            "component_type",
        ),
        (
            entry(0xC000, "a").replace("tls_map_inbox_id_string", "map"),
            "component_type",
        ),
        (
            entry(0xC000, "a").replace("'allow_if_admin'", "'unspecified'"),
            "delete_policy",
        ),
        (
            entry(0xC000, "a").replace("delete_policy = 'allow_if_admin'\n", ""),
            "delete_policy",
        ),
        (entry(0xC000, "a").replace("in_dms = false\n", ""), "in_dms"),
        (entry(0x1_C000, "a"), "component_id"),
        (format!("{}extra = 1\n", entry(0xC000, "a")), "extra"),
    ] {
        let error = load(std::slice::from_ref(&source)).unwrap_err();
        assert!(matches!(error, ConfigError::Invalid { .. }), "{error}");
        assert_eq!(
            error.field(),
            Some(format!("application_components[0].{key}").as_str()),
            "{source}"
        );
    }
}
