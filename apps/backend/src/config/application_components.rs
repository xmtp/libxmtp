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
    ApplicationComponentDefinition, ComponentPermissions, MAX_APPLICATION_COMPONENT_NAME_BYTES,
    MetadataPolicy, validate_application_components,
};
use xmtp_mls_common::app_data::component_id::ComponentId;
use xmtp_proto::xmtp::mls::message_contents::{ComponentType, metadata_policy::MetadataBasePolicy};

use super::ConfigError;
use crate::api;

/// One application component. Every key is required; there are no defaults,
/// because a published definition should not change once conversations use it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ApplicationComponentConfig {
    /// A number in `0xC000`–`0xFEFF`.
    #[schemars(range(min = 0xC000, max = 0xFEFF))]
    pub component_id: u16,
    /// A label and lookup key, 1 to 100 bytes. Prefix a field that holds one
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

/// The value shape of a component, one per `ComponentType`.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ComponentTypeConfig {
    Bytes,
    String,
    TlsMapBytesBytes,
    TlsMapInboxIdBytes,
    TlsSetBytes,
    TlsSetInboxId,
    TlsMapInboxIdString,
}

/// Who may insert, update, or delete a value, one per `MetadataBasePolicy`.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PolicyConfig {
    Allow,
    Deny,
    AllowIfAdmin,
    AllowIfSuperAdmin,
    AllowIfSelfOrNonMember,
}

impl From<ComponentTypeConfig> for ComponentType {
    fn from(component_type: ComponentTypeConfig) -> Self {
        match component_type {
            ComponentTypeConfig::Bytes => Self::Bytes,
            ComponentTypeConfig::String => Self::String,
            ComponentTypeConfig::TlsMapBytesBytes => Self::TlsMapBytesBytes,
            ComponentTypeConfig::TlsMapInboxIdBytes => Self::TlsMapInboxIdBytes,
            ComponentTypeConfig::TlsSetBytes => Self::TlsSetBytes,
            ComponentTypeConfig::TlsSetInboxId => Self::TlsSetInboxId,
            ComponentTypeConfig::TlsMapInboxIdString => Self::TlsMapInboxIdString,
        }
    }
}

impl From<PolicyConfig> for MetadataPolicy {
    fn from(policy: PolicyConfig) -> Self {
        let base = match policy {
            PolicyConfig::Allow => MetadataBasePolicy::Allow,
            PolicyConfig::Deny => MetadataBasePolicy::Deny,
            PolicyConfig::AllowIfAdmin => MetadataBasePolicy::AllowIfAdmin,
            PolicyConfig::AllowIfSuperAdmin => MetadataBasePolicy::AllowIfSuperAdmin,
            PolicyConfig::AllowIfSelfOrNonMember => MetadataBasePolicy::AllowIfSelfOrNonMember,
        };
        Self::Base(base.into())
    }
}

impl From<&ApplicationComponentConfig> for ApplicationComponentDefinition {
    fn from(config: &ApplicationComponentConfig) -> Self {
        Self {
            component_id: config.component_id,
            name: config.name.clone(),
            component_type: ComponentType::from(config.component_type).into(),
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
/// Errors name the entry by its position in the file, never by its contents.
// implements: CONF-078
pub(super) fn validate(components: &[ApplicationComponentConfig]) -> Result<(), ConfigError> {
    let definitions: Vec<ApplicationComponentDefinition> =
        components.iter().map(Into::into).collect();
    validate_application_components(&definitions)
        .map_err(|(index, reason)| ConfigError::ApplicationComponent { index, reason })?;
    match components
        .iter()
        .position(|component| ComponentId::from_well_known_name(&component.name).is_some())
    {
        Some(index) => Err(ConfigError::WellKnownComponentName { index }),
        None => Ok(()),
    }
}

/// The published catalogue, sorted by `component_id` so every client builds
/// the same snapshot whatever order the file lists them in.
// implements: CONF-079
pub(super) fn published(
    components: &[ApplicationComponentConfig],
) -> Vec<api::ApplicationComponentDefinition> {
    let mut published: Vec<api::ApplicationComponentDefinition> = components
        .iter()
        .map(|component| ApplicationComponentDefinition::from(component).into())
        .collect();
    published.sort_by_key(|definition| definition.component_id);
    published
}

#[cfg(test)]
mod tests;
