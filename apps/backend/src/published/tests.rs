use super::*;
use crate::{
    config::{
        Config,
        application_components::{ComponentTypeConfig, PolicyConfig},
    },
    server,
};
use xmtp_proto::xmtp::mls::message_contents::{
    self as mls, ComponentType,
    metadata_policy::{Kind, MetadataBasePolicy},
};

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

trait Published {
    /// The response published for this configuration without fetched keys.
    fn published(&self) -> api::GetConfigurationResponse;
}

impl Published for ValidatedConfig {
    fn published(&self) -> api::GetConfigurationResponse {
        response(self, &[])
    }
}

fn base(policy: MetadataBasePolicy) -> mls::MetadataPolicy {
    mls::MetadataPolicy {
        kind: Some(Kind::Base(policy.into())),
    }
}

/// `server::initialize` on a configuration whose database cannot be reached.
/// A refusal that comes before storage returns at once; one that comes after
/// would wait on the connection, so the timeout fails the test.
async fn initialize_without_storage(
    config: ValidatedConfig,
) -> Result<crate::Backend, Box<dyn std::error::Error + Send + Sync>> {
    let closed = std::net::TcpListener::bind("127.0.0.1:0")?.local_addr()?;
    let mut config = config.into_config();
    config.database.url = format!("postgres://xmtp:xmtp@{closed}/xmtp");
    xmtp_common::time::timeout(
        std::time::Duration::from_secs(10),
        server::initialize(config.validate()?),
    )
    .await
    .map_err(|_| "initialize reached storage")?
}

/// The component type and policy discriminants of the configuration are the
/// wire tags of the same name, for every name the schema allows.
#[xmtp_common::test(unwrap_try = true)]
fn configured_discriminants_equal_the_wire_tags() {
    let schema = serde_json::to_value(Config::schema())?;
    for (definition, prefix) in [
        ("ComponentTypeConfig", "COMPONENT_TYPE_"),
        ("PolicyConfig", "METADATA_BASE_POLICY_"),
    ] {
        let names = schema["$defs"][definition]["enum"].as_array()?;
        assert!(!names.is_empty(), "{definition}");
        for name in names {
            let wire = format!("{prefix}{}", name.as_str()?.to_ascii_uppercase());
            let (configured, tag) = if definition == "ComponentTypeConfig" {
                (
                    serde_json::from_value::<ComponentTypeConfig>(name.clone())? as i32,
                    ComponentType::from_str_name(&wire).map(|tag| tag as i32),
                )
            } else {
                (
                    serde_json::from_value::<PolicyConfig>(name.clone())? as i32,
                    MetadataBasePolicy::from_str_name(&wire).map(|tag| tag as i32),
                )
            };
            assert_eq!(Some(configured), tag, "{definition} {name}");
        }
    }
}

/// Every type and base policy an operator can name maps to its own wire tag.
// verifies: CONF-079
#[xmtp_common::test(unwrap_try = true)]
fn every_type_and_policy_name_maps_to_its_wire_tag() {
    assert!(
        load(&[])?.published().application_components.is_empty(),
        "an empty catalogue stays empty"
    );
    let groups_only = load(&[entry(0xC000, "groups")])?.published();
    assert!(groups_only.application_components[0].in_groups);
    assert!(!groups_only.application_components[0].in_dms);
    let dms_only = load(&[entry(0xC000, "dms")
        .replace("in_groups = true", "in_groups = false")
        .replace("in_dms = false", "in_dms = true")])?
    .published();
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
        let published = load(&[source])?.published();
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
        let published = load(&[source])?.published();
        let permissions = published.application_components[0].permissions.clone()?;
        assert_eq!(
            permissions.insert_policy,
            Some(base(MetadataBasePolicy::Allow))
        );
        assert_eq!(
            permissions.update_policy,
            Some(base(MetadataBasePolicy::Deny))
        );
        assert_eq!(permissions.delete_policy, Some(base(tag)));
    }
}

/// Disabled auth publishes an empty summary.
// verifies: CONF-068
#[xmtp_common::test(unwrap_try = true)]
fn a_disabled_auth_section_publishes_an_empty_summary() {
    let config = ValidatedConfig::load_str(&format!(
        "{MINIMAL}\n[auth]\nenabled = false\naudiences = []\nissuers = []\n"
    ))?;
    assert_eq!(
        config.published().auth,
        Some(api::AuthConfiguration::default())
    );
}

/// An absent minimum client version publishes as empty, and a set one as
/// written.
#[xmtp_common::test(unwrap_try = true)]
fn the_minimum_client_version_is_published_as_written() {
    assert!(
        ValidatedConfig::load_str(MINIMAL)?
            .published()
            .min_libxmtp_version
            .is_empty()
    );
    let config =
        ValidatedConfig::load_str(&format!("{MINIMAL}min_libxmtp_version = '1.2.3-beta.1'\n"))?;
    assert_eq!(config.published().min_libxmtp_version, "1.2.3-beta.1");
}

/// The auth lists, the key list, and the chain list have no individual bound,
/// so the assembled response is what is measured, for a configuration with no
/// `[auth]` table too.
// verifies: CONF-009
#[xmtp_common::test(unwrap_try = true)]
async fn an_oversized_published_response_stops_startup() {
    let mut config: Config = toml::from_str(MINIMAL)?;
    let chains = |config: &mut Config, count: u64| {
        config.chains = (0..count)
            .map(|index| (format!("eip155:{index}"), "https://rpc.example".into()))
            .collect();
    };
    chains(&mut config, 1_000);
    check_size(&config.clone().validate()?.published())?;
    chains(&mut config, 10_000);
    let config = config.validate()?;
    // The bound is on the encoded response, not on any one list.
    assert!(
        prost::Message::encoded_len(&config.published()) > MAX_CONFIGURATION_RESPONSE_BYTES,
        "the fixture must exceed the bound"
    );
    let error = initialize_without_storage(config).await.err()?;
    assert!(
        matches!(
            error.downcast_ref::<ConfigError>(),
            Some(ConfigError::Invalid {
                field: "configuration response",
                ..
            })
        ),
        "{error}"
    );
}

/// The catalogue is part of the public answer, so it counts toward the size
/// bound that keeps that answer small.
// verifies: CONF-009
#[xmtp_common::test(unwrap_try = true)]
async fn the_catalogue_counts_toward_the_published_size_bound() {
    let entries = |count: u32| -> Vec<String> {
        (0..count)
            .map(|index| entry(0xC000 + index, &format!("USER_{index:095}")))
            .collect()
    };
    check_size(&load(&entries(400))?.published())?;
    let error = initialize_without_storage(load(&entries(600))?)
        .await
        .err()?;
    assert!(
        matches!(
            error.downcast_ref::<ConfigError>(),
            Some(ConfigError::Invalid {
                field: "configuration response",
                ..
            })
        ),
        "{error}"
    );
}
