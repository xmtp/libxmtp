//! DM registry profiles (DMS section 2). A profile lists the registry entries
//! outside the application range that a DM holds from its creation. A DM
//! keeps them for life and new installations keep joining it, so a profile is
//! never changed or removed: a change to what a DM creator writes is a new
//! profile, appended to `PROFILES`. The rows are literal so that a change to
//! the creator path cannot change a profile.

use prost::Message;
use xmtp_proto::xmtp::mls::message_contents::{
    ComponentPermissions, ComponentType,
    metadata_policy::MetadataBasePolicy::{
        self, Allow, AllowIfSelfOrNonMember, AllowIfSuperAdmin, Deny,
    },
};

use crate::app_data::{
    component_id::ComponentId,
    component_registry::{ComponentRegistry, new_component_metadata},
    creation::metadata_policy,
};

/// A component, its type from META section 2, and its insert, update and
/// delete policies.
type Row = (ComponentId, ComponentType, [MetadataBasePolicy; 3]);

const SETTING: [MetadataBasePolicy; 3] = [Allow, Allow, AllowIfSuperAdmin];
const IMMUTABLE: [MetadataBasePolicy; 3] = [AllowIfSuperAdmin, Deny, Deny];

/// Profile 1: the two fixed DM policy tables of DMS section 2.
const PROFILE_1: &[Row] = &[
    (
        ComponentId::GROUP_MEMBERSHIP,
        ComponentType::TlsMapInboxIdBytes,
        [Deny, Allow, Deny],
    ),
    (
        ComponentId::ADMIN_LIST,
        ComponentType::TlsSetInboxId,
        [Deny; 3],
    ),
    (ComponentId::GROUP_NAME, ComponentType::String, SETTING),
    (
        ComponentId::GROUP_DESCRIPTION,
        ComponentType::String,
        SETTING,
    ),
    (ComponentId::GROUP_IMAGE_URL, ComponentType::String, SETTING),
    (ComponentId::APP_DATA, ComponentType::String, SETTING),
    (
        ComponentId::MESSAGE_DISAPPEAR_FROM_NS,
        ComponentType::Bytes,
        SETTING,
    ),
    (
        ComponentId::MESSAGE_DISAPPEAR_IN_NS,
        ComponentType::Bytes,
        SETTING,
    ),
    (
        ComponentId::MIN_SUPPORTED_PROTOCOL_VERSION,
        ComponentType::String,
        SETTING,
    ),
    (
        ComponentId::COMMIT_LOG_SIGNER,
        ComponentType::Bytes,
        [AllowIfSuperAdmin; 3],
    ),
    (
        ComponentId::USER_DISPLAY_NAME,
        ComponentType::TlsMapInboxIdString,
        [AllowIfSelfOrNonMember; 3],
    ),
    (
        ComponentId::CONVERSATION_TYPE,
        ComponentType::Bytes,
        IMMUTABLE,
    ),
    (
        ComponentId::CREATOR_INBOX_ID,
        ComponentType::Bytes,
        IMMUTABLE,
    ),
    (
        ComponentId::DM_MEMBERS,
        ComponentType::TlsSetInboxId,
        IMMUTABLE,
    ),
    (ComponentId::GROUP_ROOT_KEY, ComponentType::Bytes, IMMUTABLE),
];

/// Every profile, oldest first. Append a profile; never change or remove one.
const PROFILES: &[&[Row]] = &[PROFILE_1];

/// The DM registry profiles, oldest first. A DM creator writes the last one.
pub fn dm_registry_profiles() -> Vec<ComponentRegistry> {
    PROFILES
        .iter()
        .map(|rows| {
            ComponentRegistry::from_raw_entries(rows.iter().map(
                |&(id, component_type, [insert, update, delete])| {
                    let permissions = ComponentPermissions {
                        insert_policy: Some(metadata_policy(insert)),
                        update_policy: Some(metadata_policy(update)),
                        delete_policy: Some(metadata_policy(delete)),
                    };
                    let metadata = new_component_metadata(permissions, component_type);
                    (id, metadata.encode_to_vec())
                },
            ))
        })
        .collect()
}

/// True when the entries of `registry` outside the application range are, byte
/// for byte, the entries of one of `profiles`. Raw bytes keep the unknown
/// fields that decoding would drop and a later release could read.
// implements: DMS-021
pub fn matches_dm_registry_profile(
    registry: &ComponentRegistry,
    profiles: &[ComponentRegistry],
) -> bool {
    profiles
        .iter()
        .any(|profile| protocol_entries(registry).eq(protocol_entries(profile)))
}

fn protocol_entries(registry: &ComponentRegistry) -> impl Iterator<Item = (ComponentId, &[u8])> {
    registry.raw_entries().filter(|(id, _)| !id.is_app_range())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// After a newer profile is added, a DM created with the older one still
    /// matches. A registry that is neither profile does not.
    // verifies: DMS-021
    #[xmtp_common::test(unwrap_try = true)]
    fn every_profile_matches_and_nothing_else_does() {
        let metadata = |base, component_type| {
            new_component_metadata(
                ComponentPermissions {
                    insert_policy: Some(metadata_policy(base)),
                    update_policy: Some(metadata_policy(base)),
                    delete_policy: Some(metadata_policy(base)),
                },
                component_type,
            )
        };
        let older = dm_registry_profiles().pop()?;
        let mut newer = older.clone();
        newer.set(
            ComponentId::GROUP_IMAGE,
            metadata(AllowIfSuperAdmin, ComponentType::Bytes),
        )?;
        let profiles = [older.clone(), newer.clone()];
        assert!(matches_dm_registry_profile(&older, &profiles));
        assert!(matches_dm_registry_profile(&newer, &profiles));

        let mut neither = older;
        neither.set(
            ComponentId::GROUP_MEMBERSHIP,
            metadata(Deny, ComponentType::TlsMapInboxIdBytes),
        )?;
        assert!(!matches_dm_registry_profile(&neither, &profiles));
    }
}
