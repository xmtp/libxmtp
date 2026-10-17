use super::*;
use crate::Config;
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

fn load(entries: &[String]) -> Result<Config, ConfigError> {
    Config::load_str(&format!("{MINIMAL}{}", entries.concat()))
}

/// A definition no client could register, or one that makes an ID or name
/// ambiguous, stops startup. The error names the entry's position and never
/// its contents, which may have come from the environment.
// verifies: CONF-078
#[xmtp_common::test(unwrap_try = true)]
fn an_invalid_entry_stops_startup_at_its_position() {
    load(&[entry(0xC000, "a"), entry(0xFEFF, &"b".repeat(100))])?;

    let refused = |entries: &[String], index, reason| {
        let error = load(entries).unwrap_err();
        assert!(
            matches!(&error, ConfigError::ApplicationComponent { index: i, reason: r } if *i == index && *r == reason),
            "expected {reason:?} at {index}, got {error}"
        );
        assert!(!error.to_string().contains("SECRET"), "{error}");
    };
    use ApplicationComponentError as E;
    refused(&[entry(0xBFFF, "SECRET")], 0, E::ComponentId);
    refused(&[entry(0xFF00, "SECRET")], 0, E::ComponentId);
    refused(&[entry(0xC000, "")], 0, E::Name);
    refused(&[entry(0xC000, &"SECRET".repeat(17))], 0, E::Name);
    refused(
        &[entry(0xC000, "SECRET").replace("in_groups = true", "in_groups = false")],
        0,
        E::Conversations,
    );
    refused(
        &[
            entry(0xC000, "a"),
            entry(0xC001, "b"),
            entry(0xC000, "SECRET"),
        ],
        2,
        E::DuplicateId,
    );
    refused(
        &[entry(0xC000, "SECRET"), entry(0xC001, "SECRET")],
        1,
        E::DuplicateName,
    );

    let error = load(&[entry(0xC000, "a"), entry(0xC001, "USER_DISPLAY_NAME")]).unwrap_err();
    assert!(
        matches!(error, ConfigError::WellKnownComponentName { index: 1 }),
        "{error}"
    );
    // The clash is with the exact META name only.
    load(&[entry(0xC000, "user_display_name")])?;
}

/// A type or policy outside the supported set, a missing policy, and an ID
/// wider than 16 bits are not a definition at all, so the file does not load.
// verifies: CONF-078
#[xmtp_common::test(unwrap_try = true)]
fn an_unsupported_type_or_missing_policy_does_not_parse() {
    for source in [
        entry(0xC000, "a").replace("tls_map_inbox_id_string", "unspecified"),
        entry(0xC000, "a").replace("tls_map_inbox_id_string", "map"),
        entry(0xC000, "a").replace("'allow_if_admin'", "'unspecified'"),
        entry(0xC000, "a").replace("delete_policy = 'allow_if_admin'\n", ""),
        entry(0xC000, "a").replace("in_dms = false\n", ""),
        entry(0x1_C000, "a"),
        format!("{}extra = 1\n", entry(0xC000, "a")),
    ] {
        assert!(
            matches!(load(std::slice::from_ref(&source)), Err(ConfigError::Parse)),
            "{source}"
        );
    }
}
