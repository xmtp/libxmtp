//! App-data dictionary values that are synthesized when a conversation is created.

use tls_codec::Serialize;
use xmtp_proto::xmtp::mls::message_contents::{
    ComponentPermissions, ComponentType, MembershipPolicy as MembershipPolicyProto,
    MetadataPolicy as MetadataPolicyProto, PermissionsUpdatePolicy as PermissionsUpdatePolicyProto,
    PolicySet as PolicySetProto,
    membership_policy::{BasePolicy as MembershipBasePolicy, Kind as MembershipPolicyKind},
    metadata_policy::{
        AndCondition as MetadataAndCondition, AnyCondition as MetadataAnyCondition,
        Kind as MetadataPolicyKind, MetadataBasePolicy,
    },
    permissions_update_policy::{Kind as PermissionsPolicyKind, PermissionsBasePolicy},
};

use crate::{
    app_data::{
        component_id::ComponentId,
        component_registry::{ComponentRegistry, new_component_metadata},
        migration::MigrationError,
    },
    group_mutable_metadata::MetadataField,
    inbox_id::InboxId,
    tls_set::TlsSetDelta,
};

/// Produce a populated [`ComponentRegistry`] from the legacy
/// [`PolicySetProto`]. Deterministic: every honest peer synthesizes
/// bit-identical output from the same input.
///
/// Mapping summary:
///
/// - Mutable scalar components pull insert/update from
///   `update_metadata_policy[field_name]`; a missing key defaults to
///   admin-only (super-admin for the 0x800A protocol-version floor),
///   mirroring the legacy enforcer. Delete is hardcoded super-admin-only.
///   Per-field `ComponentType` comes from
///   [`metadata_field_registry_mapping`] — disappearing-message
///   timestamps are bytes (BE-u64), the rest are utf-8 strings.
/// - `ADMIN_LIST` is constrained: insert/update from `add_admin_policy`,
///   delete from `remove_admin_policy`. All must be deny-, admin-, or
///   super-admin-gated (synthesis rejects otherwise).
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

pub(crate) fn build_registry(
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
    policy_set
        .update_permissions_policy
        .as_ref()
        .ok_or(MigrationError::MissingPolicyField(
            "update_permissions_policy",
        ))?;

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
    // `update_metadata_policy[field]`; delete is always super-admin-only.
    // Per-field `ComponentType` comes from the mapping — strings for
    // free-form text, bytes for the BE-u64 disappearing-message
    // timestamps.
    //
    // A field ABSENT from `update_metadata_policy` must default to what the
    // legacy enforcer applied to a missing field — admin-only
    // (`group_permissions.rs`: "default to admin only for fields with
    // missing policies"), NOT `Allow`. The protocol-version floor defaults
    // to super-admin instead, matching every preconfigured PolicySet
    // (`default_map`/`default_policy`/`policy_admin_only`), so a group that
    // predates the field can't be wedged by a non-super-admin raising the
    // monotonic 0x800A floor. Present keys are carried verbatim, so a DM's
    // stored `Allow` (from `dm_map`) is preserved rather than tightened.
    for (field, component_id, component_type) in metadata_field_registry_mapping() {
        let default_base = if matches!(field, MetadataField::MinimumSupportedProtocolVersion) {
            MetadataBasePolicy::AllowIfSuperAdmin
        } else {
            MetadataBasePolicy::AllowIfAdmin
        };
        let policy = policy_set
            .update_metadata_policy
            .get(field.as_str())
            .cloned()
            .unwrap_or_else(|| metadata_policy(default_base));

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
///
/// The `(ComponentId, ComponentType)` pairs in this table must agree
/// with the static dispatch table at
/// [`super::registry_table::WELL_KNOWN`] — pinned by the unit test
/// `metadata_field_mapping_agrees_with_dispatch_table` below.
pub(crate) fn metadata_field_registry_mapping()
-> &'static [(MetadataField, ComponentId, ComponentType)] {
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

pub(crate) fn metadata_policy(base: MetadataBasePolicy) -> MetadataPolicyProto {
    MetadataPolicyProto {
        kind: Some(MetadataPolicyKind::Base(base as i32)),
    }
}

/// Convert a legacy `add_admin_policy` / `remove_admin_policy` (typed
/// as `PermissionsUpdatePolicy` on the wire) into the `MetadataPolicy`
/// that gates `ADMIN_LIST` insert/update/delete on the new side.
/// Only deny, admin, or super-admin base policies are allowed: combinators
/// or any other base value would silently break the constrained-component
/// check in [`ComponentRegistry::validate_metadata`].
pub(crate) fn admin_list_policy_to_metadata_policy(
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
            Ok(PermissionsBasePolicy::Deny) => Ok(metadata_policy(MetadataBasePolicy::Deny)),
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
/// Combinators map recursively so the registry preserves their legacy
/// meaning. Unknown base values fail loud rather than silently collapsing
/// to Deny.
pub(crate) fn membership_policy_to_metadata_policy(
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
        Some(MembershipPolicyKind::AndCondition(condition)) if !condition.policies.is_empty() => {
            Ok(MetadataPolicyProto {
                kind: Some(MetadataPolicyKind::AndCondition(MetadataAndCondition {
                    policies: condition
                        .policies
                        .iter()
                        .map(membership_policy_to_metadata_policy)
                        .collect::<Result<_, _>>()?,
                })),
            })
        }
        Some(MembershipPolicyKind::AnyCondition(condition)) if !condition.policies.is_empty() => {
            Ok(MetadataPolicyProto {
                kind: Some(MetadataPolicyKind::AnyCondition(MetadataAnyCondition {
                    policies: condition
                        .policies
                        .iter()
                        .map(membership_policy_to_metadata_policy)
                        .collect::<Result<_, _>>()?,
                })),
            })
        }
        _ => Err(MigrationError::UnknownMembershipPolicy(None)),
    }
}

/// Translate a legacy `GroupMutableMetadata` attribute string into the
/// AppData wire bytes for that component.
///
/// - `MESSAGE_DISAPPEAR_FROM_NS` / `MESSAGE_DISAPPEAR_IN_NS`: the legacy
///   attribute is a decimal-stringified `i64`; emit 8 big-endian bytes.
/// - String-typed attributes (group name/description/url/app data/min
///   version): emit the raw UTF-8 bytes unchanged.
pub fn encode_metadata_attribute_value(
    component_id: ComponentId,
    legacy_value: &str,
) -> Result<Vec<u8>, MigrationError> {
    match component_id {
        ComponentId::COMMIT_LOG_SIGNER => {
            let raw = hex::decode(legacy_value).map_err(|error| {
                MigrationError::InvalidCommitLogSignerHex {
                    reason: error.to_string(),
                }
            })?;
            let expected = xmtp_cryptography::configuration::ED25519_KEY_LENGTH;
            if raw.len() != expected {
                return Err(MigrationError::InvalidCommitLogSignerLength {
                    expected,
                    actual: raw.len(),
                });
            }
            Ok(raw)
        }
        ComponentId::MESSAGE_DISAPPEAR_FROM_NS => {
            parse_disappearing_i64("messageDisappearFromNS", legacy_value)
        }
        ComponentId::MESSAGE_DISAPPEAR_IN_NS => {
            parse_disappearing_i64("messageDisappearInNS", legacy_value)
        }
        _ => Ok(legacy_value.as_bytes().to_vec()),
    }
}

pub(crate) fn parse_disappearing_i64(
    field: &'static str,
    legacy_value: &str,
) -> Result<Vec<u8>, MigrationError> {
    let n: i64 = legacy_value
        .parse()
        .map_err(
            |err: std::num::ParseIntError| MigrationError::InvalidDisappearingTimestamp {
                field,
                value: legacy_value.to_string(),
                reason: err.to_string(),
            },
        )?;
    Ok(n.to_be_bytes().to_vec())
}

/// Encode hex inbox ids as a `TlsSet<InboxId>`. Must stay byte-identical
/// to the bridge's `encode_inbox_id_set` or byte-compare validation
/// fails.
/// Encode an inbox-id set as a **bootstrap wire delta**: a
/// `TlsSetDelta<InboxId>` of all-`Insert` mutations. The wire is
/// always a delta; bootstrap is the case where the prior set is
/// empty, so every mutation is an `Insert`. The dict stores the
/// materialized `TlsSet` snapshot; receivers translate wire → dict
/// via [`apply_wire_bytes`].
pub(crate) fn encode_inbox_id_set(inbox_ids_hex: &[String]) -> Result<Vec<u8>, MigrationError> {
    let ids: Vec<InboxId> = inbox_ids_hex
        .iter()
        .map(|s| InboxId::from_hex(s))
        .collect::<Result<Vec<_>, _>>()?;
    // Sort so the wire bytes are deterministic across senders. The
    // canonical-subset validator byte-compares this output against
    // the actual proposal, so non-determinism here would let an
    // honest sender's payload mismatch the validator's expectation.
    let mut sorted_ids: Vec<InboxId> = ids;
    sorted_ids.sort();
    sorted_ids.dedup();
    let mut delta: TlsSetDelta<InboxId> = TlsSetDelta::new();
    for id in sorted_ids {
        delta = delta.insert(id);
    }
    Ok(delta.tls_serialize_detached()?)
}

/// Encode the DM's two members as a **bootstrap wire delta**: a
/// `TlsSetDelta<InboxId>` of all-`Insert` mutations. (DM_MEMBERS is
/// declared as `ComponentType::TlsSetInboxId`; the dict stores the
/// materialized `TlsSet` snapshot.)
pub(crate) fn encode_dm_members(
    dm: &crate::group_metadata::DmMembers<xmtp_id::InboxId>,
) -> Result<Vec<u8>, MigrationError> {
    // Decode first, then compare on InboxId — hex strings can differ
    // only in case ("ABC..." vs "abc...") and still represent the same
    // inbox id. Self-DMs would otherwise slip past a string-compare
    // and `TlsSet` would silently collapse to one element, losing
    // fidelity.
    let one_str: &str = dm.member_one_inbox_id.as_ref();
    let two_str: &str = dm.member_two_inbox_id.as_ref();
    let one = InboxId::from_hex(one_str)?;
    let two = InboxId::from_hex(two_str)?;
    if one == two {
        // Include both raw inputs so case-divergent self-references
        // ("ABC..." vs "abc...") are visible in logs without having
        // to reproduce.
        return Err(MigrationError::DmMembersSelfReference(format!(
            "{} (member_one={one_str}, member_two={two_str})",
            one.to_hex(),
        )));
    }
    // Sort for deterministic wire bytes (validator byte-compare).
    let (a, b) = if one <= two { (one, two) } else { (two, one) };
    let delta = TlsSetDelta::<InboxId>::new().insert(a).insert(b);
    Ok(delta.tls_serialize_detached()?)
}

/// `CONVERSATION_TYPE` payload codec: 4-byte big-endian `i32` matching
/// `xmtp_proto::xmtp::mls::message_contents::ConversationType`.
/// Fixed-width simplifies byte-compare validation.
pub(crate) fn encode_conversation_type(value: i32) -> Vec<u8> {
    value.to_be_bytes().to_vec()
}

/// Inverse of [`encode_conversation_type`]. Test-only by design: the
/// receiver-side validator byte-compares the sender's CONVERSATION_TYPE
/// payload against [`encode_conversation_type`]'s output without ever
/// decoding it — equal bytes are semantically equal because the codec
/// is fixed-width. Decode is only needed to round-trip-test the codec
/// itself, so it stays gated behind `#[cfg(test)]` rather than leaking
/// into production callers that might be tempted to re-decode (and
/// then have to handle a wrong-length error path that the validator
/// already rules out via byte-compare).
#[cfg(test)]
pub(crate) fn decode_conversation_type(bytes: &[u8]) -> Result<i32, MigrationError> {
    let arr: [u8; 4] = bytes
        .try_into()
        .map_err(|_| MigrationError::ConversationTypePayloadLength(bytes.len()))?;
    Ok(i32::from_be_bytes(arr))
}

/// The immutable values for a new conversation.
pub enum InitialGroupKind<'a> {
    /// A group, including sync and oneshot groups.
    Group {
        conversation_type: xmtp_proto::types::ConversationType,
        oneshot_message: Option<&'a xmtp_proto::xmtp::mls::message_contents::OneshotMessage>,
    },
    /// A DM between the creator and this inbox.
    Dm { target_inbox_id: &'a str },
}

/// Build the complete dictionary stored in epoch zero.
/// Collection values are stored snapshots, not proposal deltas.
// implements: META-018, PERM-002
pub fn initial_dictionary(
    kind: InitialGroupKind<'_>,
    policy_set: &PolicySetProto,
    opts: &crate::group::GroupMetadataOptions,
    creator_inbox_id: &str,
    commit_log_signer: Option<&[u8]>,
) -> Result<openmls::extensions::AppDataDictionary, MigrationError> {
    use crate::tls_set::TlsSet;
    use openmls::extensions::AppDataDictionary;
    use prost::Message as _;
    use std::collections::BTreeMap;
    use xmtp_proto::types::ConversationType;
    use xmtp_proto::xmtp::mls::message_contents::{
        GroupMembershipEntry,
        group_membership_entry::{V1, Version},
    };

    let creator = InboxId::from_hex(creator_inbox_id)?;
    let (conversation_type, dm_target, oneshot_message) = match kind {
        InitialGroupKind::Group {
            conversation_type,
            oneshot_message,
        } => (conversation_type, None, oneshot_message),
        InitialGroupKind::Dm { target_inbox_id } => {
            let target = InboxId::from_hex(target_inbox_id)?;
            if target == creator {
                return Err(MigrationError::DmMembersSelfReference(creator.to_hex()));
            }
            (ConversationType::Dm, Some(target), None)
        }
    };
    let registry = build_registry(policy_set, dm_target.is_some(), oneshot_message.is_some())?;
    let mut dictionary = AppDataDictionary::new();
    dictionary.insert(
        ComponentId::COMPONENT_REGISTRY.as_u16(),
        registry.to_bytes()?,
    );
    let empty = TlsSet::<InboxId>::new().tls_serialize_detached()?;
    dictionary.insert(ComponentId::ADMIN_LIST.as_u16(), empty.clone());
    let super_admins = if dm_target.is_some() {
        empty
    } else {
        [creator]
            .into_iter()
            .collect::<TlsSet<_>>()
            .tls_serialize_detached()?
    };
    dictionary.insert(ComponentId::SUPER_ADMIN_LIST.as_u16(), super_admins);
    let membership = BTreeMap::from([(
        creator,
        GroupMembershipEntry {
            version: Some(Version::V1(V1 {
                sequence_id: 0,
                failed_installations: vec![],
            })),
        },
    )]);
    dictionary.insert(
        ComponentId::GROUP_MEMBERSHIP.as_u16(),
        super::migration::encode_group_membership_dict(&membership)?,
    );
    for (id, value) in [
        (ComponentId::GROUP_NAME, opts.name.as_ref()),
        (ComponentId::GROUP_DESCRIPTION, opts.description.as_ref()),
        (ComponentId::GROUP_IMAGE_URL, opts.image_url_square.as_ref()),
        (ComponentId::APP_DATA, opts.app_data.as_ref()),
    ] {
        if let Some(value) = value {
            dictionary.insert(id.as_u16(), encode_metadata_attribute_value(id, value)?);
        }
    }
    if let Some(settings) = &opts.message_disappearing_settings {
        dictionary.insert(
            ComponentId::MESSAGE_DISAPPEAR_FROM_NS.as_u16(),
            settings.from_ns.to_be_bytes().to_vec(),
        );
        dictionary.insert(
            ComponentId::MESSAGE_DISAPPEAR_IN_NS.as_u16(),
            settings.in_ns.to_be_bytes().to_vec(),
        );
    }
    if let Some(signer) = commit_log_signer {
        let expected = xmtp_cryptography::configuration::ED25519_KEY_LENGTH;
        if signer.len() != expected {
            return Err(MigrationError::InvalidCommitLogSignerLength {
                expected,
                actual: signer.len(),
            });
        }
        dictionary.insert(ComponentId::COMMIT_LOG_SIGNER.as_u16(), signer.to_vec());
    }
    dictionary.insert(
        ComponentId::MIN_SUPPORTED_PROTOCOL_VERSION.as_u16(),
        xmtp_configuration::PROPOSALS_MIN_PROTOCOL_VERSION
            .as_bytes()
            .to_vec(),
    );
    let conversation_type: xmtp_proto::xmtp::mls::message_contents::ConversationType =
        conversation_type.into();
    dictionary.insert(
        ComponentId::CONVERSATION_TYPE.as_u16(),
        encode_conversation_type(conversation_type as i32),
    );
    dictionary.insert(
        ComponentId::CREATOR_INBOX_ID.as_u16(),
        creator.tls_serialize_detached()?,
    );
    if let Some(target) = dm_target {
        let members: TlsSet<_> = [creator, target].into_iter().collect();
        dictionary.insert(
            ComponentId::DM_MEMBERS.as_u16(),
            members.tls_serialize_detached()?,
        );
    }
    if let Some(message) = oneshot_message {
        dictionary.insert(
            ComponentId::ONESHOT_MESSAGE.as_u16(),
            message.encode_to_vec(),
        );
    }
    Ok(dictionary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use prost::Message as _;
    use tls_codec::Serialize;
    use xmtp_proto::{
        types::ConversationType,
        xmtp::mls::message_contents::{
            MembershipPolicy, PermissionsUpdatePolicy,
            membership_policy::{BasePolicy as MembershipBasePolicy, Kind as MembershipPolicyKind},
            permissions_update_policy::{Kind as PermissionsPolicyKind, PermissionsBasePolicy},
        },
    };

    fn policy_set() -> PolicySetProto {
        let membership = MembershipPolicy {
            kind: Some(MembershipPolicyKind::Base(
                MembershipBasePolicy::Allow as i32,
            )),
        };
        let admin = PermissionsUpdatePolicy {
            kind: Some(PermissionsPolicyKind::Base(
                PermissionsBasePolicy::AllowIfAdmin as i32,
            )),
        };
        let super_admin = PermissionsUpdatePolicy {
            kind: Some(PermissionsPolicyKind::Base(
                PermissionsBasePolicy::AllowIfSuperAdmin as i32,
            )),
        };
        PolicySetProto {
            add_member_policy: Some(membership.clone()),
            remove_member_policy: Some(membership),
            update_metadata_policy: Default::default(),
            add_admin_policy: Some(admin.clone()),
            remove_admin_policy: Some(admin),
            update_permissions_policy: Some(super_admin),
        }
    }

    fn inbox(byte: u8) -> String {
        hex::encode([byte; 32])
    }

    #[xmtp_common::test(unwrap_try = true)]
    // verifies: META-018, PERM-002
    fn initial_group_dictionary_seeds_required_and_optional_values() {
        let creator = inbox(1);
        let options = crate::group::GroupMetadataOptions {
            name: Some("name".into()),
            description: Some("description".into()),
            image_url_square: Some("image".into()),
            app_data: Some("app data".into()),
            message_disappearing_settings: Some(
                crate::group_mutable_metadata::MessageDisappearingSettings::new(7, 9),
            ),
        };
        let oneshot = xmtp_proto::xmtp::mls::message_contents::OneshotMessage::default();
        let signer = [3; xmtp_cryptography::configuration::ED25519_KEY_LENGTH];
        let dictionary = initial_dictionary(
            InitialGroupKind::Group {
                conversation_type: ConversationType::Group,
                oneshot_message: Some(&oneshot),
            },
            &policy_set(),
            &options,
            &creator,
            Some(&signer),
        )?;

        let creator_id = InboxId::from_hex(&creator)?;
        let empty_admins = crate::tls_set::TlsSet::<InboxId>::new().tls_serialize_detached()?;
        assert_eq!(
            dictionary.get(&ComponentId::ADMIN_LIST.as_u16()),
            Some(empty_admins.as_slice())
        );
        let super_admins = [creator_id]
            .into_iter()
            .collect::<crate::tls_set::TlsSet<_>>()
            .tls_serialize_detached()?;
        assert_eq!(
            dictionary.get(&ComponentId::SUPER_ADMIN_LIST.as_u16()),
            Some(super_admins.as_slice())
        );
        let membership = super::super::migration::decode_group_membership_dict(
            dictionary
                .get(&ComponentId::GROUP_MEMBERSHIP.as_u16())
                .unwrap(),
        )?;
        assert!(matches!(
            membership.get(&creator_id).unwrap().version,
            Some(xmtp_proto::xmtp::mls::message_contents::group_membership_entry::Version::V1(ref entry))
                if entry.sequence_id == 0 && entry.failed_installations.is_empty()
        ));
        for (id, value) in [
            (ComponentId::GROUP_NAME, b"name".as_slice()),
            (ComponentId::GROUP_DESCRIPTION, b"description".as_slice()),
            (ComponentId::GROUP_IMAGE_URL, b"image".as_slice()),
            (ComponentId::APP_DATA, b"app data".as_slice()),
            (ComponentId::MESSAGE_DISAPPEAR_FROM_NS, &7_i64.to_be_bytes()),
            (ComponentId::MESSAGE_DISAPPEAR_IN_NS, &9_i64.to_be_bytes()),
        ] {
            assert_eq!(dictionary.get(&id.as_u16()), Some(value));
        }
        assert_eq!(
            dictionary.get(&ComponentId::COMMIT_LOG_SIGNER.as_u16()),
            Some(signer.as_slice())
        );
        assert_eq!(
            dictionary.get(&ComponentId::MIN_SUPPORTED_PROTOCOL_VERSION.as_u16()),
            Some(xmtp_configuration::PROPOSALS_MIN_PROTOCOL_VERSION.as_bytes())
        );
        let conversation_type = encode_conversation_type(
            xmtp_proto::xmtp::mls::message_contents::ConversationType::Group as i32,
        );
        assert_eq!(
            dictionary.get(&ComponentId::CONVERSATION_TYPE.as_u16()),
            Some(conversation_type.as_slice())
        );
        let serialized_creator_id = creator_id.tls_serialize_detached()?;
        assert_eq!(
            dictionary.get(&ComponentId::CREATOR_INBOX_ID.as_u16()),
            Some(serialized_creator_id.as_slice())
        );
        let encoded_oneshot = oneshot.encode_to_vec();
        assert_eq!(
            dictionary.get(&ComponentId::ONESHOT_MESSAGE.as_u16()),
            Some(encoded_oneshot.as_slice())
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    // verifies: META-018, DMS-002
    fn initial_dm_dictionary_has_empty_admin_sets_and_dm_members() {
        let creator = inbox(1);
        let target = inbox(2);
        let dictionary = initial_dictionary(
            InitialGroupKind::Dm {
                target_inbox_id: &target,
            },
            &policy_set(),
            &crate::group::GroupMetadataOptions::default(),
            &creator,
            None,
        )?;

        let creator_id = InboxId::from_hex(&creator)?;
        let target_id = InboxId::from_hex(&target)?;
        let empty = crate::tls_set::TlsSet::<InboxId>::new().tls_serialize_detached()?;
        assert_eq!(
            dictionary.get(&ComponentId::ADMIN_LIST.as_u16()),
            Some(empty.as_slice())
        );
        assert_eq!(
            dictionary.get(&ComponentId::SUPER_ADMIN_LIST.as_u16()),
            Some(empty.as_slice())
        );
        let dm_members = [creator_id, target_id]
            .into_iter()
            .collect::<crate::tls_set::TlsSet<_>>()
            .tls_serialize_detached()?;
        assert_eq!(
            dictionary.get(&ComponentId::DM_MEMBERS.as_u16()),
            Some(dm_members.as_slice())
        );
        let conversation_type = encode_conversation_type(
            xmtp_proto::xmtp::mls::message_contents::ConversationType::Dm as i32,
        );
        assert_eq!(
            dictionary.get(&ComponentId::CONVERSATION_TYPE.as_u16()),
            Some(conversation_type.as_slice())
        );
        let serialized_creator_id = creator_id.tls_serialize_detached()?;
        assert_eq!(
            dictionary.get(&ComponentId::CREATOR_INBOX_ID.as_u16()),
            Some(serialized_creator_id.as_slice())
        );
        for id in [
            ComponentId::GROUP_NAME,
            ComponentId::GROUP_DESCRIPTION,
            ComponentId::GROUP_IMAGE_URL,
            ComponentId::APP_DATA,
            ComponentId::MESSAGE_DISAPPEAR_FROM_NS,
            ComponentId::MESSAGE_DISAPPEAR_IN_NS,
            ComponentId::COMMIT_LOG_SIGNER,
            ComponentId::ONESHOT_MESSAGE,
        ] {
            assert!(!dictionary.contains(&id.as_u16()));
        }
    }
}
