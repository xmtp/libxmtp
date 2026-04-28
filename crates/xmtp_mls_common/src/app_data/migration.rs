//! Shared types, errors, and encoding helpers for the app-data
//! migration synthesis path.

use std::collections::BTreeMap;

use prost::Message as _;
use tls_codec::{Deserialize, Serialize, VLBytes};
use xmtp_proto::xmtp::mls::message_contents::{
    ComponentPermissions, ComponentType, MembershipPolicy as MembershipPolicyProto,
    MetadataPolicy as MetadataPolicyProto, PermissionsUpdatePolicy as PermissionsUpdatePolicyProto,
    PolicySet as PolicySetProto,
    membership_policy::{BasePolicy as MembershipBasePolicy, Kind as MembershipPolicyKind},
    metadata_policy::{Kind as MetadataPolicyKind, MetadataBasePolicy},
    permissions_update_policy::{Kind as PermissionsPolicyKind, PermissionsBasePolicy},
};

use crate::{
    app_data::{
        component_id::ComponentId,
        component_registry::{ComponentRegistry, new_component_metadata},
    },
    group_mutable_metadata::MetadataField,
    inbox_id::{InboxId, InboxIdError},
    tls_map::{TlsMapDelta, TlsMapMutation},
};
use xmtp_proto::xmtp::mls::message_contents::GroupMembershipEntry;
/// Errors produced by the synthesis functions in this module.
#[derive(Debug, thiserror::Error)]
pub enum MigrationError {
    /// A required `PolicySet` field was `None`. Both production presets
    /// populate every field, so this only fires on corrupt input.
    #[error("legacy PolicySet is missing required policy field: {0}")]
    MissingPolicyField(&'static str),

    /// `update_metadata_policy` referenced a metadata field we don't
    /// recognize. Silently dropping it would lose permission
    /// enforcement, so synthesis fails fast.
    #[error("update_metadata_policy references unknown metadata field: {0}")]
    UnknownMetadataField(String),

    /// `add_admin_policy`/`remove_admin_policy` wasn't admin- or
    /// super-admin-gated; the constrained-component `MetadataPolicy`
    /// shape can't represent it.
    #[error("ADMIN_LIST admin policy is not admin-or-super-admin (got base={0:?})")]
    NonConstrainedAdminPolicy(Option<i32>),

    /// `update_permissions_policy` must be `AllowIfSuperAdmin` —
    /// `COMPONENT_REGISTRY` is hardcoded super-admin-only on the
    /// receiver, so any other value would silently disagree.
    #[error("update_permissions_policy must be AllowIfSuperAdmin (got {0:?})")]
    UpdatePermissionsNotSuperAdmin(Option<i32>),

    #[error("component registry error: {0}")]
    Registry(#[from] crate::app_data::component_registry::ComponentRegistryError),

    #[error("legacy mutable-metadata extension missing from group")]
    MissingMutableMetadataExtension,

    #[error("legacy group-membership extension missing from group")]
    MissingGroupMembershipExtension,

    #[error("legacy GroupMembership extension decode error: {0}")]
    GroupMembershipDecode(#[from] prost::DecodeError),

    /// Kept distinct from `GroupMembershipDecode` so incident-response
    /// greps land on the right extension type.
    #[error("legacy GroupMutablePermissionsV1 decode error: {0}")]
    GroupPermissionsDecode(prost::DecodeError),

    #[error("legacy GroupMutableMetadata decode error: {0}")]
    MutableMetadataDecode(#[from] crate::group_mutable_metadata::GroupMutableMetadataError),

    #[error("legacy GroupMetadata decode error: {0}")]
    GroupMetadataDecode(#[from] crate::group_metadata::GroupMetadataError),

    #[error("TLS codec error: {0}")]
    TlsCodec(#[from] tls_codec::Error),

    #[error("invalid inbox id: {0}")]
    InvalidInboxId(#[from] InboxIdError),

    /// A `GROUP_MEMBERSHIP` membership-policy variant we can't translate
    /// onto `MetadataPolicyProto`. Mirrors `UnknownMetadataField` —
    /// silently collapsing to Deny would lose enforcement.
    #[error("unrecognized GROUP_MEMBERSHIP policy (base={0:?})")]
    UnknownMembershipPolicy(Option<i32>),

    #[error("invalid CONVERSATION_TYPE payload length: expected 4, got {0}")]
    ConversationTypePayloadLength(usize),

    /// Legacy `GroupMetadata.dm_members` had the same inbox in both slots.
    /// `TlsSet<InboxId>` (the `DM_MEMBERS` wire encoding) dedupes by
    /// value and would silently collapse this to one element, so we
    /// fail loud instead of destroying information.
    #[error("DmMembers self-reference: both slots contain inbox id {0}")]
    DmMembersSelfReference(String),

    /// `GroupMembershipEntry` envelope decoded but the `version` oneof
    /// was unset (or set to a variant this build doesn't recognize).
    /// Treated as a hard decode failure rather than a silent skip.
    #[error("GroupMembershipEntry envelope has unknown or unset version")]
    GroupMembershipEntryUnknownVersion,

    /// A bootstrap-time `GROUP_MEMBERSHIP` delta carried a non-`Insert`
    /// mutation. Bootstrap is "delta from empty," so anything but
    /// `Insert` means the sender or fixture is malformed.
    #[error("GROUP_MEMBERSHIP bootstrap delta carried a non-Insert mutation")]
    GroupMembershipNonInsertBootstrapMutation,
}

/// Produce a populated [`ComponentRegistry`] from the legacy
/// [`PolicySetProto`]. Deterministic: every honest peer synthesizes
/// bit-identical output from the same input.
///
/// Mapping summary (see the migration plan for the full table):
///
/// - Mutable scalar components pull insert/update from
///   `update_metadata_policy[field_name]` (default Allow); delete is
///   hardcoded super-admin-only. Per-field `ComponentType` comes from
///   [`metadata_field_registry_mapping`] — disappearing-message
///   timestamps are bytes (BE-u64), the rest are utf-8 strings.
/// - `ADMIN_LIST` is constrained: insert/update from `add_admin_policy`,
///   delete from `remove_admin_policy`. All must be admin-or-
///   super-admin (synthesis rejects otherwise).
/// - `SUPER_ADMIN_LIST` and `COMPONENT_REGISTRY` are hardcoded
///   super-admin-only and not written to the registry.
/// - `GROUP_MEMBERSHIP` mirrors `add_member_policy`/`remove_member_policy`
///   for insert/delete; update is `Allow` (anyone can advance
///   installations).
/// - Immutable components: super-admin insert, deny update + delete.
pub fn synthesize_registry_from_policy_set(
    policy_set: &PolicySetProto,
) -> Result<ComponentRegistry, MigrationError> {
    // Public entry point: maximal registry (every well-known component).
    // The internal `build_registry` gates `DM_MEMBERS` / `ONESHOT_MESSAGE`
    // for receiver-side synthesis where the group may not have them —
    // those live in the immutable range and can't be removed once written.
    build_registry(policy_set, true, true)
}

fn build_registry(
    policy_set: &PolicySetProto,
    include_dm_members: bool,
    include_oneshot_message: bool,
) -> Result<ComponentRegistry, MigrationError> {
    let mut registry = ComponentRegistry::new();

    // Defensive: every top-level policy must be present.
    let add_member = policy_set
        .add_member_policy
        .as_ref()
        .ok_or(MigrationError::MissingPolicyField("add_member_policy"))?;
    let remove_member = policy_set
        .remove_member_policy
        .as_ref()
        .ok_or(MigrationError::MissingPolicyField("remove_member_policy"))?;
    let add_admin = policy_set
        .add_admin_policy
        .as_ref()
        .ok_or(MigrationError::MissingPolicyField("add_admin_policy"))?;
    let remove_admin = policy_set
        .remove_admin_policy
        .as_ref()
        .ok_or(MigrationError::MissingPolicyField("remove_admin_policy"))?;
    let update_permissions =
        policy_set
            .update_permissions_policy
            .as_ref()
            .ok_or(MigrationError::MissingPolicyField(
                "update_permissions_policy",
            ))?;

    // update_permissions_policy MUST be super-admin-only. Any other
    // value would be silently ignored because COMPONENT_REGISTRY's
    // permissions are enforced in code (hardcoded super-admin-only).
    validate_update_permissions_is_super_admin(update_permissions)?;

    // Fail fast on unknown `update_metadata_policy` keys before doing
    // any work — silently dropping them would lose permission
    // enforcement.
    let known: std::collections::HashSet<&'static str> = metadata_field_registry_mapping()
        .iter()
        .map(|(f, _, _)| f.as_str())
        .collect();
    for key in policy_set.update_metadata_policy.keys() {
        if !known.contains(key.as_str()) {
            return Err(MigrationError::UnknownMetadataField(key.clone()));
        }
    }

    // Mutable scalar components: insert/update from
    // `update_metadata_policy[field]` (default Allow); delete is always
    // super-admin-only. Per-field `ComponentType` comes from the
    // mapping — strings for free-form text, bytes for the BE-u64
    // disappearing-message timestamps.
    for (field, component_id, component_type) in metadata_field_registry_mapping() {
        let policy = policy_set
            .update_metadata_policy
            .get(field.as_str())
            .cloned()
            .unwrap_or_else(|| metadata_policy(MetadataBasePolicy::Allow));

        registry.set(
            *component_id,
            new_component_metadata(
                ComponentPermissions {
                    insert_policy: Some(policy.clone()),
                    update_policy: Some(policy),
                    delete_policy: Some(metadata_policy(MetadataBasePolicy::AllowIfSuperAdmin)),
                },
                *component_type,
            ),
        )?;
    }

    // COMMIT_LOG_SIGNER: super-admin-only regardless of PolicySet
    // shape. The field DOES change post-creation (see
    // `Group::update_commit_log_signer`), but its enforcement on the
    // legacy side is implicit: `_commit_log_signer` is never present
    // in `update_metadata_policy` (neither `default_map` nor `dm_map`
    // populates it — `supported_fields()` excludes `CommitLogSigner`),
    // so the policy enforcer at `group_permissions.rs` falls through
    // to the `_`-prefix super-admin-only path. We encode that
    // implicit policy explicitly here. A malicious peer that *does*
    // ship `_commit_log_signer` inside `update_metadata_policy` will
    // surface as `UnknownMetadataField` above — fail-loud beats
    // silently downgrading enforcement.
    let super_admin = metadata_policy(MetadataBasePolicy::AllowIfSuperAdmin);
    registry.set(
        ComponentId::COMMIT_LOG_SIGNER,
        new_component_metadata(
            ComponentPermissions {
                insert_policy: Some(super_admin.clone()),
                update_policy: Some(super_admin.clone()),
                delete_policy: Some(super_admin.clone()),
            },
            ComponentType::Bytes,
        ),
    )?;

    // ADMIN_LIST (SetInboxId, constrained).
    let admin_policy = admin_list_policy_to_metadata_policy(add_admin)?;
    let remove_admin_policy = admin_list_policy_to_metadata_policy(remove_admin)?;
    registry.set(
        ComponentId::ADMIN_LIST,
        new_component_metadata(
            ComponentPermissions {
                insert_policy: Some(admin_policy.clone()),
                update_policy: Some(admin_policy),
                delete_policy: Some(remove_admin_policy),
            },
            ComponentType::TlsSetInboxId,
        ),
    )?;

    // GROUP_MEMBERSHIP (TlsMapInboxIdBytes).
    registry.set(
        ComponentId::GROUP_MEMBERSHIP,
        new_component_metadata(
            ComponentPermissions {
                insert_policy: Some(membership_policy_to_metadata_policy(add_member)?),
                update_policy: Some(metadata_policy(MetadataBasePolicy::Allow)),
                delete_policy: Some(membership_policy_to_metadata_policy(remove_member)?),
            },
            ComponentType::TlsMapInboxIdBytes,
        ),
    )?;

    // Immutable seeds: super-admin insert, deny update + delete.
    // `DM_MEMBERS` and `ONESHOT_MESSAGE` are gated on the `include_*`
    // flags — registering them for a group that doesn't have them
    // would pin their absence forever (immutable entries can't be
    // removed after write). Receiver-side synthesis gates them
    // symmetrically so byte-compare always lines up.
    let immutable_permissions = ComponentPermissions {
        insert_policy: Some(metadata_policy(MetadataBasePolicy::AllowIfSuperAdmin)),
        update_policy: Some(metadata_policy(MetadataBasePolicy::Deny)),
        delete_policy: Some(metadata_policy(MetadataBasePolicy::Deny)),
    };
    for id in [
        ComponentId::CONVERSATION_TYPE,
        ComponentId::CREATOR_INBOX_ID,
    ] {
        registry.set(
            id,
            new_component_metadata(immutable_permissions.clone(), ComponentType::Bytes),
        )?;
    }
    if include_oneshot_message {
        registry.set(
            ComponentId::ONESHOT_MESSAGE,
            new_component_metadata(immutable_permissions.clone(), ComponentType::Bytes),
        )?;
    }
    if include_dm_members {
        registry.set(
            ComponentId::DM_MEMBERS,
            new_component_metadata(immutable_permissions, ComponentType::TlsSetInboxId),
        )?;
    }

    Ok(registry)
}

/// List of (`MetadataField`, `ComponentId`, `ComponentType`) tuples the
/// registry knows about. Kept in one place so synthesis and validation
/// can iterate the same set.
///
/// The legacy `GroupMutableMetadata` stored every value as a `String`
/// (with disappearing-message timestamps stringified via `to_string()`),
/// but on the new component side the natural representation differs:
/// the disappearing-message timestamps round-trip as big-endian `u64`
/// bytes, while everything else is utf-8 text. Tagging the type here
/// drives `new_component_metadata` to register the correct
/// `ComponentType`.
fn metadata_field_registry_mapping() -> &'static [(MetadataField, ComponentId, ComponentType)] {
    &[
        (
            MetadataField::GroupName,
            ComponentId::GROUP_NAME,
            ComponentType::String,
        ),
        (
            MetadataField::Description,
            ComponentId::GROUP_DESCRIPTION,
            ComponentType::String,
        ),
        (
            MetadataField::GroupImageUrlSquare,
            ComponentId::GROUP_IMAGE_URL,
            ComponentType::String,
        ),
        (
            MetadataField::MessageDisappearFromNS,
            ComponentId::MESSAGE_DISAPPEAR_FROM_NS,
            ComponentType::Bytes,
        ),
        (
            MetadataField::MessageDisappearInNS,
            ComponentId::MESSAGE_DISAPPEAR_IN_NS,
            ComponentType::Bytes,
        ),
        (
            MetadataField::AppData,
            ComponentId::APP_DATA,
            ComponentType::String,
        ),
        (
            MetadataField::MinimumSupportedProtocolVersion,
            ComponentId::MIN_SUPPORTED_PROTOCOL_VERSION,
            ComponentType::String,
        ),
    ]
}

fn metadata_policy(base: MetadataBasePolicy) -> MetadataPolicyProto {
    MetadataPolicyProto {
        kind: Some(MetadataPolicyKind::Base(base as i32)),
    }
}

/// Convert a legacy `add_admin_policy` / `remove_admin_policy` (typed
/// as `PermissionsUpdatePolicy` on the wire) into the `MetadataPolicy`
/// that gates `ADMIN_LIST` insert/update/delete on the new side.
/// Only admin- or super-admin base policies are allowed: combinators
/// or any other base value would silently break the constrained-
/// component check in [`ComponentRegistry::validate_metadata`].
fn admin_list_policy_to_metadata_policy(
    p: &PermissionsUpdatePolicyProto,
) -> Result<MetadataPolicyProto, MigrationError> {
    match &p.kind {
        Some(PermissionsPolicyKind::Base(base)) => match PermissionsBasePolicy::try_from(*base) {
            Ok(PermissionsBasePolicy::AllowIfAdmin) => {
                Ok(metadata_policy(MetadataBasePolicy::AllowIfAdmin))
            }
            Ok(PermissionsBasePolicy::AllowIfSuperAdmin) => {
                Ok(metadata_policy(MetadataBasePolicy::AllowIfSuperAdmin))
            }
            _ => Err(MigrationError::NonConstrainedAdminPolicy(Some(*base))),
        },
        Some(PermissionsPolicyKind::AndCondition(_))
        | Some(PermissionsPolicyKind::AnyCondition(_))
        | None => Err(MigrationError::NonConstrainedAdminPolicy(None)),
    }
}

/// Convert a legacy `MembershipPolicy` to a `MetadataPolicy`.
/// `AllowIfAdminOrSuperAdmin` collapses to `AllowIfAdmin` because
/// `MetadataPolicy::AllowIfAdmin` already means "admin or super admin".
/// Combinators and unknown base values fail loud rather than silently
/// collapsing to Deny.
fn membership_policy_to_metadata_policy(
    p: &MembershipPolicyProto,
) -> Result<MetadataPolicyProto, MigrationError> {
    match &p.kind {
        Some(MembershipPolicyKind::Base(base)) => {
            let mapped = match MembershipBasePolicy::try_from(*base) {
                Ok(MembershipBasePolicy::Allow) => MetadataBasePolicy::Allow,
                Ok(MembershipBasePolicy::Deny) => MetadataBasePolicy::Deny,
                Ok(MembershipBasePolicy::AllowIfAdminOrSuperAdmin) => {
                    MetadataBasePolicy::AllowIfAdmin
                }
                Ok(MembershipBasePolicy::AllowIfSuperAdmin) => {
                    MetadataBasePolicy::AllowIfSuperAdmin
                }
                _ => return Err(MigrationError::UnknownMembershipPolicy(Some(*base))),
            };
            Ok(metadata_policy(mapped))
        }
        Some(MembershipPolicyKind::AndCondition(_))
        | Some(MembershipPolicyKind::AnyCondition(_))
        | None => Err(MigrationError::UnknownMembershipPolicy(None)),
    }
}

fn validate_update_permissions_is_super_admin(
    p: &PermissionsUpdatePolicyProto,
) -> Result<(), MigrationError> {
    match &p.kind {
        Some(PermissionsPolicyKind::Base(base)) => match PermissionsBasePolicy::try_from(*base) {
            Ok(PermissionsBasePolicy::AllowIfSuperAdmin) => Ok(()),
            _ => Err(MigrationError::UpdatePermissionsNotSuperAdmin(Some(*base))),
        },
        _ => Err(MigrationError::UpdatePermissionsNotSuperAdmin(None)),
    }
}

/// Encode the bootstrap-time `GROUP_MEMBERSHIP` payload as a
/// `TlsMapDelta<InboxId, VLBytes>` of all-`Insert` mutations — one per
/// inbox, each value a [`GroupMembershipEntry`] envelope (currently
/// always wrapping a `V1`).
///
/// Bootstrap is the first delta against an empty `TlsMap`, so all
/// mutations are inserts. Post-bootstrap updates use the same
/// `TlsMapDelta` wire format with mixed `Insert`/`Update`/`Delete`
/// mutations — same encode/decode path, no snapshot vs. delta split.
pub fn encode_group_membership_delta(
    entries: &BTreeMap<InboxId, GroupMembershipEntry>,
) -> Result<Vec<u8>, MigrationError> {
    let mut delta: TlsMapDelta<InboxId, VLBytes> = TlsMapDelta::new();
    for (inbox_id, entry) in entries {
        delta = delta.insert(*inbox_id, VLBytes::new(entry.encode_to_vec()));
    }
    Ok(delta.tls_serialize_detached()?)
}

/// Decode the bootstrap-time `GROUP_MEMBERSHIP` payload back to a
/// `BTreeMap<InboxId, GroupMembershipEntryV1>` by walking the
/// `TlsMapDelta` mutations. All mutations must be `Insert` (bootstrap
/// is delta-from-empty); anything else surfaces
/// [`MigrationError::GroupMembershipNonInsertBootstrapMutation`].
pub fn decode_group_membership_delta(
    bytes: &[u8],
) -> Result<BTreeMap<InboxId, GroupMembershipEntry>, MigrationError> {
    let delta = TlsMapDelta::<InboxId, VLBytes>::tls_deserialize_exact(bytes)?;
    let mut out: BTreeMap<InboxId, GroupMembershipEntry> = BTreeMap::new();
    for mutation in delta.mutations {
        match mutation {
            TlsMapMutation::Insert { key, value } => {
                let envelope = GroupMembershipEntry::decode(value.as_slice())?;
                if envelope.version.is_none() {
                    return Err(MigrationError::GroupMembershipEntryUnknownVersion);
                }
                out.insert(key, envelope);
            }
            TlsMapMutation::Update { .. } | TlsMapMutation::Delete { .. } => {
                return Err(MigrationError::GroupMembershipNonInsertBootstrapMutation);
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use xmtp_proto::xmtp::mls::message_contents::{
        MembershipPolicy as MembershipPolicyProto, MetadataPolicy as MetadataPolicyProto,
        PermissionsUpdatePolicy as PermissionsUpdatePolicyProto,
        group_membership_entry::{
            V1 as GroupMembershipEntryV1, Version as GroupMembershipEntryVersion,
        },
        membership_policy::{BasePolicy as MembershipBase, Kind as MembershipKind},
        metadata_policy::Kind as MetadataKind,
        permissions_update_policy::Kind as PermissionsKind,
    };

    fn allow_metadata() -> MetadataPolicyProto {
        MetadataPolicyProto {
            kind: Some(MetadataKind::Base(MetadataBasePolicy::Allow as i32)),
        }
    }
    fn allow_if_admin_metadata() -> MetadataPolicyProto {
        MetadataPolicyProto {
            kind: Some(MetadataKind::Base(MetadataBasePolicy::AllowIfAdmin as i32)),
        }
    }
    fn admin_only_perms() -> PermissionsUpdatePolicyProto {
        PermissionsUpdatePolicyProto {
            kind: Some(PermissionsKind::Base(
                PermissionsBasePolicy::AllowIfAdmin as i32,
            )),
        }
    }
    fn super_admin_only_perms() -> PermissionsUpdatePolicyProto {
        PermissionsUpdatePolicyProto {
            kind: Some(PermissionsKind::Base(
                PermissionsBasePolicy::AllowIfSuperAdmin as i32,
            )),
        }
    }
    fn allow_membership() -> MembershipPolicyProto {
        MembershipPolicyProto {
            kind: Some(MembershipKind::Base(MembershipBase::Allow as i32)),
        }
    }

    fn minimal_default_policy_set() -> PolicySetProto {
        PolicySetProto {
            add_member_policy: Some(allow_membership()),
            remove_member_policy: Some(allow_membership()),
            update_metadata_policy: std::collections::HashMap::new(),
            add_admin_policy: Some(admin_only_perms()),
            remove_admin_policy: Some(admin_only_perms()),
            update_permissions_policy: Some(super_admin_only_perms()),
        }
    }

    #[test]
    fn synthesizes_all_well_known_components() {
        let registry = synthesize_registry_from_policy_set(&minimal_default_policy_set()).unwrap();
        // Mutable scalar family.
        for (_, id, _) in metadata_field_registry_mapping() {
            assert!(registry.contains(id), "missing {}", id);
        }
        // COMMIT_LOG_SIGNER.
        assert!(registry.contains(&ComponentId::COMMIT_LOG_SIGNER));
        // ADMIN_LIST + GROUP_MEMBERSHIP.
        assert!(registry.contains(&ComponentId::ADMIN_LIST));
        assert!(registry.contains(&ComponentId::GROUP_MEMBERSHIP));
        // Immutable seeds.
        assert!(registry.contains(&ComponentId::CONVERSATION_TYPE));
        assert!(registry.contains(&ComponentId::CREATOR_INBOX_ID));
        assert!(registry.contains(&ComponentId::DM_MEMBERS));
        assert!(registry.contains(&ComponentId::ONESHOT_MESSAGE));
        // Hardcoded are NOT in the registry.
        assert!(!registry.contains(&ComponentId::SUPER_ADMIN_LIST));
        assert!(!registry.contains(&ComponentId::COMPONENT_REGISTRY));
    }

    #[test]
    fn synthesis_defaults_to_allow_for_missing_metadata_fields() {
        let registry = synthesize_registry_from_policy_set(&minimal_default_policy_set()).unwrap();
        let meta = registry.get(&ComponentId::GROUP_NAME).unwrap().unwrap();
        let perms = meta.permissions.unwrap();
        let ins = perms.insert_policy.unwrap();
        assert_eq!(ins, allow_metadata());
    }

    #[test]
    fn synthesis_uses_per_field_policy_when_present() {
        let mut ps = minimal_default_policy_set();
        ps.update_metadata_policy
            .insert("group_name".to_string(), allow_if_admin_metadata());
        let registry = synthesize_registry_from_policy_set(&ps).unwrap();
        let meta = registry.get(&ComponentId::GROUP_NAME).unwrap().unwrap();
        assert_eq!(
            meta.permissions.clone().unwrap().insert_policy.unwrap(),
            allow_if_admin_metadata()
        );
        // Description still defaults to Allow.
        let desc = registry
            .get(&ComponentId::GROUP_DESCRIPTION)
            .unwrap()
            .unwrap();
        assert_eq!(
            desc.permissions.unwrap().insert_policy.unwrap(),
            allow_metadata()
        );
    }

    #[test]
    fn synthesis_rejects_unknown_metadata_field() {
        let mut ps = minimal_default_policy_set();
        ps.update_metadata_policy
            .insert("something_new".to_string(), allow_metadata());
        let err = synthesize_registry_from_policy_set(&ps).unwrap_err();
        assert!(matches!(err, MigrationError::UnknownMetadataField(f) if f == "something_new"));
    }

    #[test]
    fn synthesis_rejects_non_super_admin_update_permissions() {
        let mut ps = minimal_default_policy_set();
        ps.update_permissions_policy = Some(admin_only_perms());
        let err = synthesize_registry_from_policy_set(&ps).unwrap_err();
        assert!(matches!(
            err,
            MigrationError::UpdatePermissionsNotSuperAdmin(_)
        ));
    }

    #[test]
    fn synthesis_admin_list_super_admin_only() {
        let mut ps = minimal_default_policy_set();
        ps.add_admin_policy = Some(super_admin_only_perms());
        ps.remove_admin_policy = Some(super_admin_only_perms());
        let registry = synthesize_registry_from_policy_set(&ps).unwrap();
        let admin = registry.get(&ComponentId::ADMIN_LIST).unwrap().unwrap();
        let perms = admin.permissions.unwrap();
        assert_eq!(
            perms.insert_policy.unwrap(),
            MetadataPolicyProto {
                kind: Some(MetadataKind::Base(
                    MetadataBasePolicy::AllowIfSuperAdmin as i32
                ))
            }
        );
    }

    #[test]
    fn synthesis_sets_correct_component_type_per_field() {
        // Disappearing-message timestamps round-trip as BE-u64 bytes;
        // every other mutable scalar is utf-8 string. Keep this test in
        // lockstep with `metadata_field_registry_mapping`.
        let registry = synthesize_registry_from_policy_set(&minimal_default_policy_set()).unwrap();
        let expected: &[(ComponentId, ComponentType)] = &[
            (ComponentId::GROUP_NAME, ComponentType::String),
            (ComponentId::GROUP_DESCRIPTION, ComponentType::String),
            (ComponentId::GROUP_IMAGE_URL, ComponentType::String),
            (ComponentId::APP_DATA, ComponentType::String),
            (
                ComponentId::MIN_SUPPORTED_PROTOCOL_VERSION,
                ComponentType::String,
            ),
            (ComponentId::MESSAGE_DISAPPEAR_FROM_NS, ComponentType::Bytes),
            (ComponentId::MESSAGE_DISAPPEAR_IN_NS, ComponentType::Bytes),
        ];
        for (id, ty) in expected {
            let meta = registry.get(id).unwrap().unwrap();
            assert_eq!(
                meta.component_type, *ty as i32,
                "wrong component_type for {id}"
            );
        }
    }

    #[test]
    fn synthesis_deterministic_bytes() {
        // Bit-identical output from two calls on the same input is the
        // foundation invariant for byte-compare validation.
        let a = synthesize_registry_from_policy_set(&minimal_default_policy_set()).unwrap();
        let b = synthesize_registry_from_policy_set(&minimal_default_policy_set()).unwrap();
        assert_eq!(a.to_bytes().unwrap(), b.to_bytes().unwrap());
    }

    #[test]
    fn membership_policy_rejects_combinator() {
        use xmtp_proto::xmtp::mls::message_contents::{
            MembershipPolicy as MembershipPolicyProto,
            membership_policy::{AndCondition as AndCondProto, Kind as MembershipKind},
        };
        let combinator = MembershipPolicyProto {
            kind: Some(MembershipKind::AndCondition(AndCondProto {
                policies: vec![],
            })),
        };
        let err = membership_policy_to_metadata_policy(&combinator).unwrap_err();
        assert!(matches!(err, MigrationError::UnknownMembershipPolicy(None)));
    }

    #[test]
    fn membership_policy_rejects_unknown_base() {
        use xmtp_proto::xmtp::mls::message_contents::{
            MembershipPolicy as MembershipPolicyProto, membership_policy::Kind as MembershipKind,
        };
        let unknown = MembershipPolicyProto {
            kind: Some(MembershipKind::Base(9999)),
        };
        let err = membership_policy_to_metadata_policy(&unknown).unwrap_err();
        assert!(matches!(
            err,
            MigrationError::UnknownMembershipPolicy(Some(9999))
        ));
    }

    #[test]
    fn group_membership_encode_round_trip() {
        let mut entries: BTreeMap<InboxId, GroupMembershipEntryV1> = BTreeMap::new();
        entries.insert(
            InboxId::from_bytes([0x01; 32]),
            GroupMembershipEntryV1 {
                sequence_id: 42,
                failed_installations: vec![vec![0xAA; 16]],
            },
        );
        entries.insert(
            InboxId::from_bytes([0x02; 32]),
            GroupMembershipEntryV1 {
                sequence_id: 99,
                failed_installations: vec![],
            },
        );
        let entries = entries
            .into_iter()
            .map(|(k, v)| {
                (
                    k,
                    GroupMembershipEntry {
                        version: Some(GroupMembershipEntryVersion::V1(v)),
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        let bytes = encode_group_membership_delta(&entries).unwrap();
        let decoded = decode_group_membership_delta(&bytes).unwrap();
        assert_eq!(decoded, entries);
    }

    #[test]
    fn admin_list_policy_rejects_combinator() {
        use xmtp_proto::xmtp::mls::message_contents::{
            PermissionsUpdatePolicy as PermissionsUpdatePolicyProto,
            permissions_update_policy::{AndCondition as AndCondProto, Kind as PermissionsKind},
        };
        let combinator = PermissionsUpdatePolicyProto {
            kind: Some(PermissionsKind::AndCondition(AndCondProto {
                policies: vec![],
            })),
        };
        let err = admin_list_policy_to_metadata_policy(&combinator).unwrap_err();
        assert!(matches!(
            err,
            MigrationError::NonConstrainedAdminPolicy(None)
        ));
    }

    #[test]
    fn decode_rejects_non_insert_bootstrap_mutation() {
        // Hand-build a delta with a Delete mutation; bootstrap is
        // delta-from-empty, so anything but Insert must surface as
        // GroupMembershipNonInsertBootstrapMutation.
        let mut delta: TlsMapDelta<InboxId, VLBytes> = TlsMapDelta::new();
        delta = delta.delete(InboxId::from_bytes([0x03; 32]));
        let bytes = delta.tls_serialize_detached().unwrap();
        let err = decode_group_membership_delta(&bytes).unwrap_err();
        assert!(matches!(
            err,
            MigrationError::GroupMembershipNonInsertBootstrapMutation
        ));
    }

    #[test]
    fn decode_rejects_unset_version() {
        // Encode an envelope with `version: None` and verify the decoder
        // surfaces GroupMembershipEntryUnknownVersion rather than
        // silently treating the entry as empty.
        let envelope = GroupMembershipEntry { version: None };
        let mut delta: TlsMapDelta<InboxId, VLBytes> = TlsMapDelta::new();
        delta = delta.insert(
            InboxId::from_bytes([0x04; 32]),
            VLBytes::new(envelope.encode_to_vec()),
        );
        let bytes = delta.tls_serialize_detached().unwrap();
        let err = decode_group_membership_delta(&bytes).unwrap_err();
        assert!(matches!(
            err,
            MigrationError::GroupMembershipEntryUnknownVersion
        ));
    }
}
