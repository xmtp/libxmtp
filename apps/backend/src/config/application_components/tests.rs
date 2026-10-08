use super::*;
use crate::config::Config;
use xmtp_configuration::ApplicationComponentError;
use xmtp_proto::xmtp::mls::message_contents::{self as mls, metadata_policy::Kind};

const MINIMAL: &str =
    "[database]\nurl = 'postgres://localhost/xmtp'\n[server]\nidentifier = 'org.xmtp.test'\n";

/// One `[[application_components]]` table with every key set.
fn entry(component_id: u32, name: &str) -> String {
    format!(
        "\n[[application_components]]\ncomponent_id = {component_id}\nname = '{name}'\n\
         component_type = 'tls_map_inbox_id_string'\ninsert_policy = 'allow_if_self_or_non_member'\n\
         update_policy = 'allow_if_self_or_non_member'\ndelete_policy = 'allow_if_admin'\n\
         in_groups = true\nin_dms = false\n"
    )
}

fn load(entries: &[String]) -> Result<Config, ConfigError> {
    Config::load_str(&format!("{MINIMAL}{}", entries.concat()))
}

fn base(policy: MetadataBasePolicy) -> mls::MetadataPolicy {
    mls::MetadataPolicy {
        kind: Some(Kind::Base(policy.into())),
    }
}

/// Every type and base policy an operator can name maps to its own wire tag.
// verifies: CONF-079
#[xmtp_common::test(unwrap_try = true)]
fn every_type_and_policy_name_maps_to_its_wire_tag() {
    assert!(
        load(&[])?
            .configuration_response(&[])
            .application_components
            .is_empty(),
        "an empty catalogue stays empty"
    );
    let groups_only = load(&[entry(0xC000, "groups")])?.configuration_response(&[]);
    assert!(groups_only.application_components[0].in_groups);
    assert!(!groups_only.application_components[0].in_dms);
    let dms_only = load(&[entry(0xC000, "dms")
        .replace("in_groups = true", "in_groups = false")
        .replace("in_dms = false", "in_dms = true")])?
    .configuration_response(&[]);
    assert!(!dms_only.application_components[0].in_groups);
    assert!(dms_only.application_components[0].in_dms);

    for (name, tag) in [
        ("bytes", ComponentType::Bytes),
        ("string", ComponentType::String),
        ("tls_map_bytes_bytes", ComponentType::TlsMapBytesBytes),
        ("tls_map_inbox_id_bytes", ComponentType::TlsMapInboxIdBytes),
        ("tls_set_bytes", ComponentType::TlsSetBytes),
        ("tls_set_inbox_id", ComponentType::TlsSetInboxId),
        (
            "tls_map_inbox_id_string",
            ComponentType::TlsMapInboxIdString,
        ),
    ] {
        let source = entry(0xC000, "field").replace("tls_map_inbox_id_string", name);
        let published = load(&[source])?.configuration_response(&[]);
        assert_eq!(
            published.application_components[0].component_type,
            tag as i32
        );
    }
    for (name, tag) in [
        ("allow", MetadataBasePolicy::Allow),
        ("deny", MetadataBasePolicy::Deny),
        ("allow_if_admin", MetadataBasePolicy::AllowIfAdmin),
        (
            "allow_if_super_admin",
            MetadataBasePolicy::AllowIfSuperAdmin,
        ),
        (
            "allow_if_self_or_non_member",
            MetadataBasePolicy::AllowIfSelfOrNonMember,
        ),
    ] {
        let source = entry(0xC000, "field").replace("'allow_if_admin'", &format!("'{name}'"));
        let published = load(&[source])?.configuration_response(&[]);
        let permissions = published.application_components[0].permissions.clone()?;
        assert_eq!(permissions.delete_policy, Some(base(tag)));
    }
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

/// The catalogue is part of the public answer, so it counts toward the size
/// bound that keeps that answer small.
// verifies: CONF-009
#[xmtp_common::test(unwrap_try = true)]
fn the_catalogue_counts_toward_the_published_size_bound() {
    let entries = |count: u32| -> Vec<String> {
        (0..count)
            .map(|index| entry(0xC000 + index, &format!("USER_{index:095}")))
            .collect()
    };
    load(&entries(400))?;
    let error = load(&entries(600)).unwrap_err();
    assert!(
        matches!(
            error,
            ConfigError::Invalid {
                field: "configuration response",
                ..
            }
        ),
        "{error}"
    );
}
