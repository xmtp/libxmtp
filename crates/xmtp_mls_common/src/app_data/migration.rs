//! Shared types, errors, and encoding helpers for the app-data
//! migration synthesis path.

use std::collections::BTreeMap;

use prost::Message as _;
use tls_codec::{Deserialize, Serialize, VLBytes};

use crate::{
    app_data::component_registry::ComponentRegistryError,
    inbox_id::{InboxId, InboxIdError},
    tls_map::{TlsMap, TlsMapDelta, TlsMapMutation},
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

    /// `add_admin_policy`/`remove_admin_policy` wasn't deny-, admin-,
    /// or super-admin-gated; the constrained-component `MetadataPolicy`
    /// shape can't represent it.
    #[error("ADMIN_LIST admin policy is not deny, admin, or super-admin (got base={0:?})")]
    NonConstrainedAdminPolicy(Option<i32>),

    #[error("component registry error: {0}")]
    Registry(#[from] ComponentRegistryError),

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

    /// Legacy `GroupMembership.failed_installations` carried an entry
    /// whose length isn't 32 bytes (the Ed25519 installation-key size).
    /// Either the legacy state is corrupt or the wire shape changed —
    /// either way, fail loud rather than silently admit a malformed
    /// installation ID into the validator's allow-set.
    #[error("legacy failed_installations entry has invalid length: expected 32, got {0}")]
    InvalidFailedInstallationLength(usize),

    /// A bootstrap-time `GROUP_MEMBERSHIP` wire delta carried a
    /// non-`Insert` mutation. Bootstrap is always "delta from empty,"
    /// so anything but `Insert` means the sender is malformed.
    #[error("GROUP_MEMBERSHIP bootstrap delta carried a non-Insert mutation")]
    GroupMembershipNonInsertBootstrapMutation,

    /// A bootstrap-time `GROUP_MEMBERSHIP` wire delta carried two
    /// `Insert` mutations for the same inbox id. The wire shape is
    /// `TlsMapDelta` which permits this in principle, but bootstrap
    /// must be a deterministic snapshot — the duplicate would let the
    /// sender's queue order diverge from honest receivers.
    #[error("GROUP_MEMBERSHIP bootstrap delta has duplicate Insert for inbox {0}")]
    GroupMembershipDuplicateInbox(String),

    /// Legacy `MessageDisappearFromNS` / `MessageDisappearInNS` GMM
    /// attribute didn't parse as a base-10 string of an `i64`. The
    /// legacy reader at `MessageDisappearingSettings` returns
    /// `MissingExtension` for this case; bootstrap synthesis fails
    /// loud instead so the migrated dict can't silently drop a
    /// configured value.
    #[error("legacy {field} attribute is not a base-10 i64 string (got {value:?}): {reason}")]
    InvalidDisappearingTimestamp {
        field: &'static str,
        value: String,
        reason: String,
    },

    /// Legacy `CommitLogSigner` GMM attribute wasn't valid hex.
    #[error("legacy CommitLogSigner attribute is not valid hex: {reason}")]
    InvalidCommitLogSignerHex { reason: String },

    /// Legacy `CommitLogSigner` decoded to the wrong number of bytes —
    /// the AppData wire shape is the raw 32-byte Ed25519 private key.
    #[error("legacy CommitLogSigner length: expected {expected}, got {actual}")]
    InvalidCommitLogSignerLength { expected: usize, actual: usize },
}

/// Encode `GROUP_MEMBERSHIP` for the bootstrap **wire payload** as a
/// `TlsMapDelta<InboxId, VLBytes>` of all-`Insert` mutations — one
/// per inbox, each value a [`GroupMembershipEntry`] envelope
/// (currently always wrapping a `V1`).
///
/// The wire is always a delta. Bootstrap is the case where the prior
/// dict state is empty, so every mutation is an `Insert`. Steady-
/// state updates use the same `TlsMapDelta` wire shape with mixed
/// `Insert` / `Update` / `Delete` mutations describing only the
/// inboxes that changed (see `update_group_membership.rs`).
pub fn encode_group_membership_delta(
    entries: &BTreeMap<InboxId, GroupMembershipEntry>,
) -> Result<Vec<u8>, MigrationError> {
    let mut delta: TlsMapDelta<InboxId, VLBytes> = TlsMapDelta::new();
    for (inbox_id, entry) in entries {
        delta = delta.insert(*inbox_id, VLBytes::new(entry.encode_to_vec()));
    }
    Ok(delta.tls_serialize_detached()?)
}

/// Decode a `GROUP_MEMBERSHIP` **wire payload** (a
/// `TlsMapDelta<InboxId, VLBytes>` of all-`Insert` mutations against
/// an empty prior — bootstrap shape) back to a
/// `BTreeMap<InboxId, GroupMembershipEntry>`. Used by the bootstrap
/// validator to inspect the proposal payload.
pub fn decode_group_membership_delta(
    bytes: &[u8],
) -> Result<BTreeMap<InboxId, GroupMembershipEntry>, MigrationError> {
    let delta = TlsMapDelta::<InboxId, VLBytes>::tls_deserialize_exact(bytes)?;
    let mut out: BTreeMap<InboxId, GroupMembershipEntry> = BTreeMap::new();
    for mutation in delta.mutations {
        let (key, value) = match mutation {
            TlsMapMutation::Insert { key, value } => (key, value),
            TlsMapMutation::Update { .. } | TlsMapMutation::Delete { .. } => {
                return Err(MigrationError::GroupMembershipNonInsertBootstrapMutation);
            }
        };
        let envelope = GroupMembershipEntry::decode(value.as_slice())?;
        if envelope.version.is_none() {
            return Err(MigrationError::GroupMembershipEntryUnknownVersion);
        }
        if out.insert(key, envelope).is_some() {
            return Err(MigrationError::GroupMembershipDuplicateInbox(key.to_hex()));
        }
    }
    Ok(out)
}

/// Encode `GROUP_MEMBERSHIP` **dict-storage bytes** as a
/// `TlsMap<InboxId, VLBytes>` snapshot. The dict always holds the
/// raw map as state; this encoder is the symmetric of
/// [`decode_group_membership_dict`] and is used by tests and by
/// callers that need to construct expected dict bytes (the runtime
/// path goes through `apply_app_data_update_payload`, which produces
/// the same snapshot from a wire delta).
pub fn encode_group_membership_dict(
    entries: &BTreeMap<InboxId, GroupMembershipEntry>,
) -> Result<Vec<u8>, MigrationError> {
    let mut map: TlsMap<InboxId, VLBytes> = TlsMap::new();
    for (inbox_id, entry) in entries {
        map.insert(*inbox_id, VLBytes::new(entry.encode_to_vec()))
            .map_err(|e| {
                MigrationError::TlsCodec(tls_codec::Error::EncodingError(e.to_string()))
            })?;
    }
    Ok(map.tls_serialize_detached()?)
}

/// Decode `GROUP_MEMBERSHIP` **dict-storage bytes** (a
/// `TlsMap<InboxId, VLBytes>` snapshot) back to a `BTreeMap<InboxId,
/// GroupMembershipEntry>`. Used by readers walking the AppData
/// dictionary post-bootstrap.
pub fn decode_group_membership_dict(
    bytes: &[u8],
) -> Result<BTreeMap<InboxId, GroupMembershipEntry>, MigrationError> {
    let snapshot = TlsMap::<InboxId, VLBytes>::tls_deserialize_exact(bytes)?;
    let mut out: BTreeMap<InboxId, GroupMembershipEntry> = BTreeMap::new();
    for (key, value) in snapshot.into_iter() {
        let envelope = GroupMembershipEntry::decode(value.as_slice())?;
        if envelope.version.is_none() {
            return Err(MigrationError::GroupMembershipEntryUnknownVersion);
        }
        out.insert(key, envelope);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app_data::{
            component_id::ComponentId,
            creation::{
                admin_list_policy_to_metadata_policy, decode_conversation_type,
                encode_conversation_type, membership_policy_to_metadata_policy,
                metadata_field_registry_mapping, metadata_policy,
                synthesize_registry_from_policy_set,
            },
        },
        group_mutable_metadata::MetadataField,
    };
    use xmtp_proto::xmtp::mls::message_contents::{
        ComponentType, MembershipPolicy as MembershipPolicyProto,
        MetadataPolicy as MetadataPolicyProto,
        PermissionsUpdatePolicy as PermissionsUpdatePolicyProto, PolicySet as PolicySetProto,
        group_membership_entry::{
            V1 as GroupMembershipEntryV1, Version as GroupMembershipEntryVersion,
        },
        membership_policy::{BasePolicy as MembershipBase, Kind as MembershipKind},
        metadata_policy::{
            AndCondition as MetadataAndCondition, AnyCondition as MetadataAnyCondition,
            Kind as MetadataKind, MetadataBasePolicy,
        },
        permissions_update_policy::{Kind as PermissionsKind, PermissionsBasePolicy},
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
    fn allow_if_super_admin_metadata() -> MetadataPolicyProto {
        MetadataPolicyProto {
            kind: Some(MetadataKind::Base(
                MetadataBasePolicy::AllowIfSuperAdmin as i32,
            )),
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
    fn deny_perms() -> PermissionsUpdatePolicyProto {
        PermissionsUpdatePolicyProto {
            kind: Some(PermissionsKind::Base(PermissionsBasePolicy::Deny as i32)),
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

    #[xmtp_common::test(unwrap_try = true)]
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

    #[xmtp_common::test(unwrap_try = true)]
    fn synthesis_defaults_to_admin_for_missing_metadata_fields() {
        // A field absent from the legacy `update_metadata_policy` map must
        // mirror the legacy enforcer's missing-field fallback (admin-only),
        // NOT `Allow` — otherwise migrating a group that predates the field
        // silently downgrades it to anyone-editable.
        let registry = synthesize_registry_from_policy_set(&minimal_default_policy_set()).unwrap();
        let meta = registry.get(&ComponentId::GROUP_NAME).unwrap().unwrap();
        let perms = meta.permissions.unwrap();
        assert_eq!(perms.insert_policy.unwrap(), allow_if_admin_metadata());
        assert_eq!(perms.update_policy.unwrap(), allow_if_admin_metadata());
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn synthesis_defaults_min_version_floor_to_super_admin_when_missing() {
        // The monotonic 0x800A protocol-version floor is a group-wedge lever:
        // a group whose stored PolicySet omits it (created before the field
        // existed, or every DM created before it) must NOT let a non-super-
        // admin raise it, or any member could pause the group permanently.
        let registry = synthesize_registry_from_policy_set(&minimal_default_policy_set()).unwrap();
        let meta = registry
            .get(&ComponentId::MIN_SUPPORTED_PROTOCOL_VERSION)
            .unwrap()
            .unwrap();
        let perms = meta.permissions.unwrap();
        assert_eq!(
            perms.insert_policy.unwrap(),
            allow_if_super_admin_metadata()
        );
        assert_eq!(
            perms.update_policy.unwrap(),
            allow_if_super_admin_metadata()
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn synthesis_preserves_a_stored_min_version_policy() {
        // The super-admin default only fills a MISSING key; a stored floor
        // policy (e.g. a DM's `Allow` from `dm_map`) is carried verbatim so
        // migration stays faithful rather than unfaithfully tightening it.
        let mut ps = minimal_default_policy_set();
        ps.update_metadata_policy.insert(
            MetadataField::MinimumSupportedProtocolVersion
                .as_str()
                .to_string(),
            allow_metadata(),
        );
        let registry = synthesize_registry_from_policy_set(&ps).unwrap();
        let meta = registry
            .get(&ComponentId::MIN_SUPPORTED_PROTOCOL_VERSION)
            .unwrap()
            .unwrap();
        assert_eq!(
            meta.permissions.unwrap().insert_policy.unwrap(),
            allow_metadata()
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
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
        // Description, still absent from the map, defaults to admin-only.
        let desc = registry
            .get(&ComponentId::GROUP_DESCRIPTION)
            .unwrap()
            .unwrap();
        assert_eq!(
            desc.permissions.unwrap().insert_policy.unwrap(),
            allow_if_admin_metadata()
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn synthesis_rejects_unknown_metadata_field() {
        let mut ps = minimal_default_policy_set();
        ps.update_metadata_policy
            .insert("something_new".to_string(), allow_metadata());
        let err = synthesize_registry_from_policy_set(&ps).unwrap_err();
        assert!(matches!(err, MigrationError::UnknownMetadataField(f) if f == "something_new"));
    }

    #[xmtp_common::test(unwrap_try = true)]
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

    #[xmtp_common::test(unwrap_try = true)]
    fn synthesis_admin_list_preserves_deny_policy() {
        let mut ps = minimal_default_policy_set();
        ps.add_admin_policy = Some(deny_perms());
        ps.remove_admin_policy = Some(deny_perms());

        let registry = synthesize_registry_from_policy_set(&ps)?;
        let permissions = registry
            .get(&ComponentId::ADMIN_LIST)?
            .expect("ADMIN_LIST is registered")
            .permissions
            .expect("ADMIN_LIST has permissions");
        assert_eq!(
            permissions.insert_policy,
            Some(metadata_policy(MetadataBasePolicy::Deny))
        );
        assert_eq!(
            permissions.delete_policy,
            Some(metadata_policy(MetadataBasePolicy::Deny))
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn metadata_field_mapping_agrees_with_dispatch_table() {
        // The `metadata_field_registry_mapping` table duplicates
        // ComponentType info that `WELL_KNOWN` now also carries (one
        // entry per `Component` impl). Pin the invariant: if a
        // future change drifts one of the tables out of sync (e.g.
        // changes a Bytes component to String only in WELL_KNOWN),
        // this test catches it before any commit goes out.
        use crate::app_data::registry_table::lookup_component;
        for (_field, component_id, expected_type) in metadata_field_registry_mapping() {
            let dispatched = lookup_component(*component_id)
                .unwrap_or_else(|| panic!("WELL_KNOWN missing entry for {component_id}"));
            assert_eq!(
                dispatched.component_type(),
                *expected_type,
                "metadata_field_registry_mapping disagrees with WELL_KNOWN for {component_id}"
            );
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
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

    #[xmtp_common::test(unwrap_try = true)]
    fn membership_policy_preserves_combinator_recursively() {
        use xmtp_proto::xmtp::mls::message_contents::{
            MembershipPolicy as MembershipPolicyProto,
            membership_policy::{AndCondition as AndCondProto, Kind as MembershipKind},
        };
        let combinator = MembershipPolicyProto {
            kind: Some(MembershipKind::AndCondition(AndCondProto {
                policies: vec![MembershipPolicyProto {
                    kind: Some(MembershipKind::AnyCondition(
                        xmtp_proto::xmtp::mls::message_contents::membership_policy::AnyCondition {
                            policies: vec![allow_membership()],
                        },
                    )),
                }],
            })),
        };
        let translated = membership_policy_to_metadata_policy(&combinator).unwrap();
        assert_eq!(
            translated,
            MetadataPolicyProto {
                kind: Some(MetadataKind::AndCondition(MetadataAndCondition {
                    policies: vec![MetadataPolicyProto {
                        kind: Some(MetadataKind::AnyCondition(MetadataAnyCondition {
                            policies: vec![allow_metadata()],
                        })),
                    }],
                })),
            }
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
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

    #[xmtp_common::test(unwrap_try = true)]
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
        // Wire round-trip: encode as bootstrap delta, decode as
        // delta. Used by sender synthesis ↔ validator.
        let wire_bytes = encode_group_membership_delta(&entries).unwrap();
        let decoded = decode_group_membership_delta(&wire_bytes).unwrap();
        assert_eq!(decoded, entries);
    }

    #[xmtp_common::test(unwrap_try = true)]
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

    #[xmtp_common::test(unwrap_try = true)]
    fn decode_wire_rejects_unset_version() {
        // Encode a wire delta with one Insert whose envelope carries
        // `version: None`. The wire decoder must surface
        // `GroupMembershipEntryUnknownVersion` rather than silently
        // treating the entry as empty.
        let envelope = GroupMembershipEntry { version: None };
        let delta = TlsMapDelta::<InboxId, VLBytes>::new().insert(
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

    #[xmtp_common::test(unwrap_try = true)]
    fn decode_wire_rejects_non_insert_mutation() {
        // Bootstrap is delta-from-empty — `Update` or `Delete` against
        // an empty prior is meaningless and must be rejected.
        let delta = TlsMapDelta::<InboxId, VLBytes>::new().delete(InboxId::from_bytes([0x05; 32]));
        let bytes = delta.tls_serialize_detached().unwrap();
        let err = decode_group_membership_delta(&bytes).unwrap_err();
        assert!(matches!(
            err,
            MigrationError::GroupMembershipNonInsertBootstrapMutation
        ));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn decode_dict_round_trips() {
        // Dict storage uses the materialized `TlsMap` snapshot, not
        // the wire delta. Verify the dict decoder reads what the dict
        // would actually contain post-apply.
        let mut snapshot: TlsMap<InboxId, VLBytes> = TlsMap::new();
        let inbox = InboxId::from_bytes([0x06; 32]);
        let envelope = GroupMembershipEntry {
            version: Some(GroupMembershipEntryVersion::V1(GroupMembershipEntryV1 {
                sequence_id: 7,
                failed_installations: vec![],
            })),
        };
        snapshot
            .insert(inbox, VLBytes::new(envelope.encode_to_vec()))
            .unwrap();
        let bytes = snapshot.tls_serialize_detached().unwrap();
        let decoded = decode_group_membership_dict(&bytes).unwrap();
        assert_eq!(decoded.len(), 1);
        assert!(decoded.contains_key(&inbox));
    }

    // ========================================================================
    // Wire-format / codec coverage for the bootstrap canonical subset
    // ========================================================================
    //
    // These pin the byte shape each component produces so a future tweak
    // to the encoder can't silently break byte-identity between sender
    // synthesis and the receiver's byte-compare validation.

    #[xmtp_common::test(unwrap_try = true)]
    fn conversation_type_codec_round_trips() {
        // 0=Unspecified, 1=Group, 2=Dm today, plus a negative to pin
        // the two's-complement representation in case the enum is ever
        // widened.
        for v in [0_i32, 1, 2, -1, i32::MAX, i32::MIN] {
            let bytes = encode_conversation_type(v);
            assert_eq!(bytes.len(), 4, "always 4 bytes");
            assert_eq!(decode_conversation_type(&bytes).unwrap(), v);
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn conversation_type_decode_rejects_wrong_length() {
        let err = decode_conversation_type(&[0, 0, 0]).unwrap_err();
        assert!(matches!(
            err,
            MigrationError::ConversationTypePayloadLength(3)
        ));
        let err = decode_conversation_type(&[0; 8]).unwrap_err();
        assert!(matches!(
            err,
            MigrationError::ConversationTypePayloadLength(8)
        ));
    }
}
