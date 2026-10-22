//! Operator-defined group metadata fields, published in `GetConfiguration`.
//!
//! Each `[[application_components]]` entry names one field that clients copy
//! into the registry of a conversation they create. Startup refuses an entry
//! no client could register, a repeated ID or name, and a name equal to a
//! well-known component name. The backend enforces nothing else: a group's
//! committed registry, not this configuration, governs what it accepts.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use xmtp_configuration::{
    ApplicationComponentDefinition, ApplicationComponentError, ComponentPermissions,
    MAX_APPLICATION_COMPONENT_NAME_BYTES, MetadataPolicy, WELL_KNOWN_COMPONENT_NAMES,
    validate_application_components,
};

use super::{ConfigError, Step, key_path};

/// One application component. Every key is required; there are no defaults,
/// because a published definition should not change once conversations use it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ApplicationComponentConfig {
    /// A number in `0xC000`–`0xFEFF`.
    #[schemars(range(min = 0xC000, max = 0xFEFF))]
    pub component_id: u16,
    /// A label and lookup key, 1 to 100 UTF-8 bytes. The schema can bound only
    /// characters, so startup checks bytes. Prefix a field that holds one
    /// value per inbox with `USER_`, as the well-known user fields are.
    #[schemars(length(min = 1, max = MAX_APPLICATION_COMPONENT_NAME_BYTES))]
    pub name: String,
    pub component_type: ComponentTypeConfig,
    pub insert_policy: PolicyConfig,
    pub update_policy: PolicyConfig,
    pub delete_policy: PolicyConfig,
    /// Register in new groups.
    pub in_groups: bool,
    /// Register in new DMs.
    pub in_dms: bool,
}

// The discriminants are the wire tags. The backend pins them to `xmtp_proto`.

/// The value shape of a component, one per `ComponentType`.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ComponentTypeConfig {
    Bytes = 1,
    String = 2,
    TlsMapBytesBytes = 3,
    TlsMapInboxIdBytes = 4,
    TlsSetBytes = 5,
    TlsSetInboxId = 6,
    TlsMapInboxIdString = 7,
}

/// Who may insert, update, or delete a value, one per `MetadataBasePolicy`.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PolicyConfig {
    Allow = 1,
    Deny = 2,
    AllowIfAdmin = 3,
    AllowIfSuperAdmin = 4,
    AllowIfSelfOrNonMember = 5,
}

impl From<PolicyConfig> for MetadataPolicy {
    fn from(policy: PolicyConfig) -> Self {
        Self::Base(policy as i32)
    }
}

impl From<&ApplicationComponentConfig> for ApplicationComponentDefinition {
    fn from(config: &ApplicationComponentConfig) -> Self {
        Self {
            component_id: config.component_id,
            name: config.name.clone(),
            component_type: config.component_type as i32,
            permissions: ComponentPermissions {
                insert: Some(config.insert_policy.into()),
                update: Some(config.update_policy.into()),
                delete: Some(config.delete_policy.into()),
            },
            in_groups: config.in_groups,
            in_dms: config.in_dms,
        }
    }
}

/// Apply the rules a client also applies, then refuse a well-known name. A
/// client accepts that clash, because a later release may add a well-known
/// name that a deployment already uses; a new deployment cannot start with one.
/// Errors name the key by the entry's position in the file, never by the
/// entry's contents.
// implements: CONF-078
pub(super) fn validate(components: &[ApplicationComponentConfig]) -> Result<(), ConfigError> {
    let definitions: Vec<ApplicationComponentDefinition> =
        components.iter().map(Into::into).collect();
    validate_application_components(&definitions).map_err(|(index, reason)| {
        ConfigError::ApplicationComponent {
            field: key(index, reason_key(reason)),
            reason,
        }
    })?;
    match components.iter().position(|component| {
        WELL_KNOWN_COMPONENT_NAMES
            .iter()
            .any(|&(_, name)| name == component.name)
    }) {
        Some(index) => Err(ConfigError::WellKnownComponentName {
            field: key(index, "name"),
        }),
        None => Ok(()),
    }
}

/// The key of entry `index` named `name`.
fn key(index: usize, name: &str) -> String {
    key_path(
        &key_path("application_components", Step::Entry(index)),
        Step::Key(name),
    )
}

/// The key a refusal names. A rule that relates two keys names the first;
/// its reason names both.
fn reason_key(reason: ApplicationComponentError) -> &'static str {
    match reason {
        ApplicationComponentError::ComponentId | ApplicationComponentError::DuplicateId => {
            "component_id"
        }
        ApplicationComponentError::Name | ApplicationComponentError::DuplicateName => "name",
        // A decoded entry always has a supported type and all three
        // policies, so these two cannot fail here.
        ApplicationComponentError::ComponentType => "component_type",
        ApplicationComponentError::Permissions => "insert_policy",
        ApplicationComponentError::Conversations => "in_groups",
    }
}

#[cfg(test)]
mod tests;
