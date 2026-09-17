//! Shared types, errors, and encoding helpers for the app-data
//! migration synthesis path.

use std::collections::{BTreeMap, BTreeSet};

use openmls::{
    extensions::Extensions,
    group::{GroupContext, MlsGroup as OpenMlsGroup},
    messages::proposals::AppDataUpdateOperationType,
};
use prost::Message as _;
use tls_codec::{Deserialize, Serialize, VLBytes};
use xmtp_proto::xmtp::mls::message_contents::{
    ComponentMetadata, GroupMembership as GroupMembershipProto, PolicySet as PolicySetProto,
};

use crate::app_data::creation::{
    build_registry, encode_conversation_type, encode_dm_members, encode_inbox_id_set,
    encode_metadata_attribute_value, metadata_field_registry_mapping,
};
use crate::{
    app_data::{
        component_id::ComponentId,
        component_registry::{ComponentRegistry, ComponentRegistryError},
    },
    group_mutable_metadata::MetadataField,
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

/// The receiver-side bootstrap expectation. The validator picks the
/// comparison strategy per component:
///
/// - [`Self::strict`] — byte-compared against the sender's commit
///   payload. Used for components whose canonical encoding is
///   deterministic by construction: raw bytes/utf-8 (metadata
///   attributes, `COMMIT_LOG_SIGNER`, `CONVERSATION_TYPE`),
///   the versioned single-`InboxId` TLS wire form (`CREATOR_INBOX_ID`),
///   and TLS-codec containers that sort their keys (`ADMIN_LIST`,
///   `SUPER_ADMIN_LIST`, `DM_MEMBERS`, `ONESHOT_MESSAGE`).
/// - [`Self::expected_registry`] — `COMPONENT_REGISTRY` is decoded
///   first, then compared per entry as a typed [`ComponentMetadata`].
///   The outer `TlsMapDelta` wrapper IS deterministic, but each
///   entry's value is a prost-encoded `ComponentMetadata`. Prost
///   tag-order emission is theoretically deterministic, but
///   byte-compare is brittle against future proto evolution (newly
///   optional fields, default-value elision differences across
///   encoder versions or language bindings) and produces useless
///   diffs ("byte 47 differs"). Decoded compare side-steps both.
/// - [`Self::membership_sequence_ids`] — `GROUP_MEMBERSHIP`'s
///   `failed_installations` is sender-authoritative (the migrator
///   partitions per inbox by walking identity-update history, so
///   different honest senders may legitimately disagree on bytes),
///   so the validator only checks per-inbox `sequence_id`.
/// - [`Self::allowed_failed_installations`] — bounds the universe of
///   installation IDs the sender may legally place into ANY per-inbox
///   `failed_installations`. Drawn from the legacy
///   `GroupMembership.failed_installations` flat list, with each entry
///   length-checked to 32 bytes (Ed25519 installation key). The sender
///   is allowed to drop entries (e.g., when the owning inbox can't be
///   determined) but not to add ones the legacy state never contained.
///   Validator semantics: every per-inbox `failed_installations`
///   entry must be 32 bytes AND present in this set.
#[derive(Debug, Clone, PartialEq)]
pub struct CanonicalBootstrapExpectation {
    pub strict: BTreeMap<ComponentId, (AppDataUpdateOperationType, Vec<u8>)>,
    pub expected_registry: BTreeMap<ComponentId, ComponentMetadata>,
    pub membership_sequence_ids: BTreeMap<InboxId, u64>,
    pub allowed_failed_installations: BTreeSet<[u8; 32]>,
}

/// Compute the [`CanonicalBootstrapExpectation`] from a pre-flip
/// group's state. **Sync, fully local** — no API calls — so every
/// honest receiver produces bit-identical output.
///
/// **ENCODING FREEZE.** The `strict` map this produces is byte-compared
/// by every fielded validator against incoming bootstrap commits, and
/// groups migrate lazily — so for any input an already-shipped receiver
/// can encounter, this encoding is permanent. The
/// `golden_bootstrap_synthesis_*` tests pin it byte-for-byte. An
/// encoder change is only shippable together with a
/// `PROPOSALS_MIN_PROTOCOL_VERSION` bump (below-floor receivers pause
/// via the floor check in `validate_bootstrap_commit` instead of
/// byte-comparing), and the old expectation logic must keep validating
/// bootstraps produced by older senders.
pub fn synthesize_canonical_subset_for_validation(
    mls_group: &OpenMlsGroup,
) -> Result<CanonicalBootstrapExpectation, MigrationError> {
    synthesize_canonical_subset_from_extensions(mls_group.extensions())
}

/// Extensions-only variant of [`synthesize_canonical_subset_for_validation`].
/// Lets tests exercise synthesis without standing up a real MLS group.
pub fn synthesize_canonical_subset_from_extensions(
    extensions: &Extensions<GroupContext>,
) -> Result<CanonicalBootstrapExpectation, MigrationError> {
    let gmm: crate::group_mutable_metadata::GroupMutableMetadata = extensions.try_into()?;
    let registry = synthesize_registry_from_extensions(extensions)?;
    let legacy_membership = extract_legacy_group_membership(extensions)?;
    let legacy_metadata = crate::group_metadata::GroupMetadata::try_from(extensions)?;

    let mut strict: BTreeMap<ComponentId, (AppDataUpdateOperationType, Vec<u8>)> = BTreeMap::new();

    // COMPONENT_REGISTRY: decoded per-entry compare (see
    // `CanonicalBootstrapExpectation` doc — bytes inside each entry are
    // prost-encoded and brittle to byte-compare).
    let mut expected_registry: BTreeMap<ComponentId, ComponentMetadata> = BTreeMap::new();
    for entry in registry.iter() {
        let (id, meta) = entry?;
        expected_registry.insert(id, meta);
    }

    // Bytes/String metadata attributes. Skip fields that aren't set in
    // the legacy GMM — the typed component encoders reject empty input
    // for the fixed-length flavours (MESSAGE_DISAPPEAR_* expects 8 BE
    // bytes, COMMIT_LOG_SIGNER expects 32) and emitting absent
    // entries here would only pollute the dict with values readers
    // would surface as `MissingExtension` anyway.
    for (field, component_id, _) in metadata_field_registry_mapping() {
        if let Some(s) = gmm.attributes.get(field.as_str()) {
            strict.insert(
                *component_id,
                (
                    AppDataUpdateOperationType::Update,
                    encode_metadata_attribute_value(*component_id, s)?,
                ),
            );
        }
    }

    // COMMIT_LOG_SIGNER lives in the same GMM attributes map. The
    // legacy form is hex-encoded; the AppData wire form is the raw
    // 32-byte private key.
    if let Some(hex_str) = gmm.attributes.get(MetadataField::CommitLogSigner.as_str()) {
        let raw = hex::decode(hex_str).map_err(|e| MigrationError::InvalidCommitLogSignerHex {
            reason: e.to_string(),
        })?;
        if raw.len() != xmtp_cryptography::configuration::ED25519_KEY_LENGTH {
            return Err(MigrationError::InvalidCommitLogSignerLength {
                actual: raw.len(),
                expected: xmtp_cryptography::configuration::ED25519_KEY_LENGTH,
            });
        }
        strict.insert(
            ComponentId::COMMIT_LOG_SIGNER,
            (AppDataUpdateOperationType::Update, raw),
        );
    }

    // ADMIN_LIST / SUPER_ADMIN_LIST: hex-decode inbox-id strings and
    // serialize as TlsSet<InboxId> — matches the bridge encoder.
    strict.insert(
        ComponentId::ADMIN_LIST,
        (
            AppDataUpdateOperationType::Update,
            encode_inbox_id_set(&gmm.admin_list)?,
        ),
    );
    strict.insert(
        ComponentId::SUPER_ADMIN_LIST,
        (
            AppDataUpdateOperationType::Update,
            encode_inbox_id_set(&gmm.super_admin_list)?,
        ),
    );

    // Immutable seeds. Route the shared `ConversationType` through
    // its `From<_> for ConversationTypeProto` impl before casting to
    // i32 — the two enums share variants today but are *separate*
    // types with their own discriminants. Direct `as i32` on the shared
    // enum would silently drift if either side renumbers. Mirrors the
    // pattern in `group_metadata.rs::TryFrom<GroupMetadata> for Vec<u8>`.
    let conversation_type_proto: xmtp_proto::xmtp::mls::message_contents::ConversationType =
        legacy_metadata.conversation_type.into();
    strict.insert(
        ComponentId::CONVERSATION_TYPE,
        (
            AppDataUpdateOperationType::Update,
            encode_conversation_type(conversation_type_proto as i32),
        ),
    );
    // CREATOR_INBOX_ID rides the same versioned `InboxId` wire form
    // (`varint(version) || 32-byte payload`) every other inbox-id-bearing
    // component on the new path uses, so the bytes round-trip through
    // the same decoder.
    strict.insert(
        ComponentId::CREATOR_INBOX_ID,
        (
            AppDataUpdateOperationType::Update,
            InboxId::from_hex(&legacy_metadata.creator_inbox_id)?.tls_serialize_detached()?,
        ),
    );
    if let Some(dm) = &legacy_metadata.dm_members {
        strict.insert(
            ComponentId::DM_MEMBERS,
            (AppDataUpdateOperationType::Update, encode_dm_members(dm)?),
        );
    }
    if let Some(oneshot) = &legacy_metadata.oneshot_message {
        strict.insert(
            ComponentId::ONESHOT_MESSAGE,
            (AppDataUpdateOperationType::Update, oneshot.encode_to_vec()),
        );
    }

    // GROUP_MEMBERSHIP: per-inbox sequence-id map keyed by [`InboxId`]
    // (matches the `TlsMap<InboxId, VLBytes>` wire format).
    let mut membership_sequence_ids: BTreeMap<InboxId, u64> = BTreeMap::new();
    for (inbox_id_hex, seq) in legacy_membership.members.iter() {
        let inbox_id = InboxId::from_hex(inbox_id_hex)?;
        membership_sequence_ids.insert(inbox_id, *seq);
    }

    // Bound the universe of installation IDs the sender may legally
    // emit into ANY per-inbox `failed_installations`. Each entry must
    // be 32 bytes (Ed25519 installation-key size) — fail loud on
    // anything else rather than silently admit it to the allow-set.
    // Set semantics: the legacy field is `repeated bytes` so duplicates
    // are possible but irrelevant for subset membership checks.
    let mut allowed_failed_installations: BTreeSet<[u8; 32]> = BTreeSet::new();
    for raw in &legacy_membership.failed_installations {
        let key: [u8; 32] = raw
            .as_slice()
            .try_into()
            .map_err(|_| MigrationError::InvalidFailedInstallationLength(raw.len()))?;
        allowed_failed_installations.insert(key);
    }

    Ok(CanonicalBootstrapExpectation {
        strict,
        expected_registry,
        membership_sequence_ids,
        allowed_failed_installations,
    })
}

/// Build a registry tailored to the legacy state in `extensions` —
/// gates `DM_MEMBERS` / `ONESHOT_MESSAGE` on `GroupMetadata` presence so
/// the registry bytes line up with the per-component entries the
/// receiver will see in the bootstrap commit.
fn synthesize_registry_from_extensions(
    extensions: &Extensions<GroupContext>,
) -> Result<ComponentRegistry, MigrationError> {
    let policy_set = extract_legacy_policy_set(extensions)?;

    let legacy_metadata = crate::group_metadata::GroupMetadata::try_from(extensions)?;
    build_registry(
        &policy_set,
        legacy_metadata.dm_members.is_some(),
        legacy_metadata.oneshot_message.is_some(),
    )
}

fn extract_legacy_policy_set(
    extensions: &Extensions<GroupContext>,
) -> Result<PolicySetProto, MigrationError> {
    let policy_set_bytes = find_unknown_extension(
        extensions,
        xmtp_configuration::GROUP_PERMISSIONS_EXTENSION_ID,
    )
    .ok_or(MigrationError::MissingPolicyField(
        "group_permissions extension",
    ))?;
    let permissions_proto =
        xmtp_proto::xmtp::mls::message_contents::GroupMutablePermissionsV1::decode(
            policy_set_bytes.as_slice(),
        )
        .map_err(MigrationError::GroupPermissionsDecode)?;
    permissions_proto
        .policies
        .ok_or(MigrationError::MissingPolicyField("policies"))
}

fn find_unknown_extension(extensions: &Extensions<GroupContext>, id: u16) -> Option<&Vec<u8>> {
    use openmls::extensions::{Extension, UnknownExtension};
    extensions.iter().find_map(|extension| match extension {
        Extension::Unknown(eid, UnknownExtension(data)) if *eid == id => Some(data),
        _ => None,
    })
}

fn extract_legacy_group_membership(
    extensions: &Extensions<GroupContext>,
) -> Result<GroupMembershipProto, MigrationError> {
    let bytes = find_unknown_extension(
        extensions,
        xmtp_configuration::GROUP_MEMBERSHIP_EXTENSION_ID,
    )
    .ok_or(MigrationError::MissingGroupMembershipExtension)?;
    Ok(GroupMembershipProto::decode(bytes.as_slice())?)
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
    use crate::inbox_id::INBOX_ID_BYTE_LEN;
    use crate::{
        app_data::creation::{
            admin_list_policy_to_metadata_policy, decode_conversation_type,
            membership_policy_to_metadata_policy, metadata_policy,
            synthesize_registry_from_policy_set,
        },
        tls_set::TlsSetDelta,
    };
    use xmtp_proto::xmtp::mls::message_contents::{
        ComponentType, MembershipPolicy as MembershipPolicyProto,
        MetadataPolicy as MetadataPolicyProto,
        PermissionsUpdatePolicy as PermissionsUpdatePolicyProto,
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
    fn synthesis_deterministic_bytes() {
        // Bit-identical output from two calls on the same input is the
        // foundation invariant for byte-compare validation.
        let a = synthesize_registry_from_policy_set(&minimal_default_policy_set()).unwrap();
        let b = synthesize_registry_from_policy_set(&minimal_default_policy_set()).unwrap();
        assert_eq!(a.to_bytes().unwrap(), b.to_bytes().unwrap());
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

    fn hex_inbox(tag: u8) -> String {
        hex::encode([tag; INBOX_ID_BYTE_LEN])
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn encode_inbox_id_set_emits_bootstrap_wire_delta() {
        // Two inbox ids → `TlsSetDelta<InboxId>` of all-`Insert`
        // mutations (bootstrap = delta-from-empty against an empty
        // prior). The wire is always a delta; bootstrap is the case
        // where every mutation happens to be an Insert. Each `InboxId`
        // on the wire is `varint(0) || 32 raw bytes`.
        use crate::tls_set::TlsSetMutation;
        let ids = vec![hex_inbox(0xAA), hex_inbox(0xBB)];
        let bytes = encode_inbox_id_set(&ids).unwrap();
        let delta = TlsSetDelta::<InboxId>::tls_deserialize_exact(&bytes)
            .expect("decodes as TlsSetDelta<InboxId>");
        assert_eq!(delta.mutations.len(), 2);
        // Sorted ascending so the wire bytes are deterministic.
        for (mutation, expected_tag) in delta.mutations.iter().zip([0xAA, 0xBB]) {
            match mutation {
                TlsSetMutation::Insert(id) => {
                    assert_eq!(id.as_bytes(), &[expected_tag; INBOX_ID_BYTE_LEN]);
                }
                other => panic!("expected Insert, got {other:?}"),
            }
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn encode_inbox_id_set_rejects_bad_hex() {
        let err = encode_inbox_id_set(&["not-hex".to_string()]).unwrap_err();
        assert!(matches!(err, MigrationError::InvalidInboxId(_)));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn encode_dm_members_produces_two_insert_delta() {
        use crate::tls_set::TlsSetMutation;
        let dm = crate::group_metadata::DmMembers {
            member_one_inbox_id: hex_inbox(0xCC),
            member_two_inbox_id: hex_inbox(0xDD),
        };
        let bytes = encode_dm_members(&dm).unwrap();
        let delta = TlsSetDelta::<InboxId>::tls_deserialize_exact(&bytes).unwrap();
        assert_eq!(delta.mutations.len(), 2);
        for (mutation, expected_tag) in delta.mutations.iter().zip([0xCC, 0xDD]) {
            match mutation {
                TlsSetMutation::Insert(id) => {
                    assert_eq!(id.as_bytes(), &[expected_tag; INBOX_ID_BYTE_LEN]);
                }
                other => panic!("expected Insert, got {other:?}"),
            }
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn encode_dm_members_rejects_self_reference() {
        // `TlsSet<InboxId>` dedupes by value. A self-DM (both slots
        // identical) would silently collapse to a one-element set —
        // fail loud instead so the fidelity loss is visible.
        let id = hex_inbox(0xEE);
        let dm = crate::group_metadata::DmMembers {
            member_one_inbox_id: id.clone(),
            member_two_inbox_id: id.clone(),
        };
        let err = encode_dm_members(&dm).unwrap_err();
        let MigrationError::DmMembersSelfReference(msg) = err else {
            panic!("expected DmMembersSelfReference, got {err:?}");
        };
        // Message carries the canonical hex plus both raw inputs.
        assert!(msg.starts_with(&id), "missing canonical hex: {msg}");
        assert!(msg.contains(&format!("member_one={id}")), "{msg}");
        assert!(msg.contains(&format!("member_two={id}")), "{msg}");
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn encode_dm_members_rejects_case_divergent_self_reference() {
        // Hex encoding is case-insensitive, so two strings with
        // different cases can name the same inbox id. A naive
        // string-compare would miss this and `TlsSet` would silently
        // collapse the duplicate. Decode-then-compare catches it, and
        // the error carries both raw inputs so the case divergence is
        // visible in logs without reproducing.
        let lower = hex_inbox(0xEE);
        let upper = lower.to_ascii_uppercase();
        assert_ne!(lower, upper, "test premise: strings must differ");
        let dm = crate::group_metadata::DmMembers {
            member_one_inbox_id: lower.clone(),
            member_two_inbox_id: upper.clone(),
        };
        let err = encode_dm_members(&dm).unwrap_err();
        let MigrationError::DmMembersSelfReference(msg) = err else {
            panic!("expected DmMembersSelfReference, got {err:?}");
        };
        // Canonical (lowercase) hex first, then both raw inputs as
        // observed — proves we kept fidelity for log inspection.
        assert!(msg.starts_with(&lower), "missing canonical hex: {msg}");
        assert!(msg.contains(&format!("member_one={lower}")), "{msg}");
        assert!(msg.contains(&format!("member_two={upper}")), "{msg}");
    }

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

    // ========================================================================
    // End-to-end canonical-subset coverage via synthetic Extensions
    // ========================================================================

    /// Synthetic `Extensions<GroupContext>` with the four legacy
    /// extensions that bootstrap synthesis reads.
    fn build_test_extensions(
        gmm: crate::group_mutable_metadata::GroupMutableMetadata,
        policy_set: PolicySetProto,
        membership: xmtp_proto::xmtp::mls::message_contents::GroupMembership,
        metadata: crate::group_metadata::GroupMetadata,
    ) -> Extensions<GroupContext> {
        use openmls::extensions::{Extension, Metadata, UnknownExtension};
        use xmtp_configuration::{
            GROUP_MEMBERSHIP_EXTENSION_ID, GROUP_PERMISSIONS_EXTENSION_ID,
            MUTABLE_METADATA_EXTENSION_ID,
        };
        use xmtp_proto::xmtp::mls::message_contents::GroupMutablePermissionsV1;

        let gmm_bytes: Vec<u8> = gmm.try_into().unwrap();
        let permissions_bytes = GroupMutablePermissionsV1 {
            policies: Some(policy_set),
        }
        .encode_to_vec();
        let membership_bytes = membership.encode_to_vec();
        let metadata_bytes: Vec<u8> = metadata.try_into().unwrap();

        Extensions::from_vec(vec![
            Extension::Unknown(MUTABLE_METADATA_EXTENSION_ID, UnknownExtension(gmm_bytes)),
            Extension::Unknown(
                GROUP_PERMISSIONS_EXTENSION_ID,
                UnknownExtension(permissions_bytes),
            ),
            Extension::Unknown(
                GROUP_MEMBERSHIP_EXTENSION_ID,
                UnknownExtension(membership_bytes),
            ),
            Extension::ImmutableMetadata(Metadata::new(metadata_bytes)),
        ])
        .unwrap()
    }

    fn default_gmm() -> crate::group_mutable_metadata::GroupMutableMetadata {
        crate::group_mutable_metadata::GroupMutableMetadata::new(
            std::collections::HashMap::new(),
            Vec::new(),
            Vec::new(),
        )
    }

    fn empty_membership() -> xmtp_proto::xmtp::mls::message_contents::GroupMembership {
        xmtp_proto::xmtp::mls::message_contents::GroupMembership {
            members: std::collections::HashMap::new(),
            failed_installations: vec![],
        }
    }

    fn plain_group_metadata() -> crate::group_metadata::GroupMetadata {
        crate::group_metadata::GroupMetadata::new(
            xmtp_proto::types::ConversationType::Group,
            hex_inbox(0x11),
            None,
            None,
        )
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn canonical_subset_empty_group_omits_optional_seeds() {
        // Non-DM, non-oneshot group. DM_MEMBERS and ONESHOT_MESSAGE
        // must be absent from BOTH the strict byte-compare map and the
        // registry bytes — any asymmetry between them would trip the
        // receiver-side byte-compare check.
        let exts = build_test_extensions(
            default_gmm(),
            minimal_default_policy_set(),
            empty_membership(),
            plain_group_metadata(),
        );
        let subset = synthesize_canonical_subset_from_extensions(&exts).unwrap();

        // Strict table contains every always-present component, but NOT
        // COMPONENT_REGISTRY (which compares semantically via
        // `expected_registry`).
        assert!(!subset.strict.contains_key(&ComponentId::COMPONENT_REGISTRY));
        assert!(subset.strict.contains_key(&ComponentId::ADMIN_LIST));
        assert!(subset.strict.contains_key(&ComponentId::SUPER_ADMIN_LIST));
        assert!(subset.strict.contains_key(&ComponentId::CONVERSATION_TYPE));
        assert!(subset.strict.contains_key(&ComponentId::CREATOR_INBOX_ID));

        // Optional seeds gated on presence:
        // - DM_MEMBERS / ONESHOT_MESSAGE: only present for DM / oneshot groups.
        // - The `GroupMutableMetadata`-backed bytes/string attributes
        //   (GROUP_NAME, GROUP_DESCRIPTION, GROUP_IMAGE_URL,
        //   MESSAGE_DISAPPEAR_*, COMMIT_LOG_SIGNER, APP_DATA,
        //   MIN_SUPPORTED_PROTOCOL_VERSION) are seeded only when the
        //   legacy GMM has a value for them. An empty GMM produces no
        //   seed entries, matching the legacy reader semantics where
        //   "absent" surfaces as `MissingExtension` rather than as an
        //   empty value.
        assert!(!subset.strict.contains_key(&ComponentId::DM_MEMBERS));
        assert!(!subset.strict.contains_key(&ComponentId::ONESHOT_MESSAGE));
        assert!(!subset.strict.contains_key(&ComponentId::GROUP_NAME));
        assert!(
            !subset
                .strict
                .contains_key(&ComponentId::MESSAGE_DISAPPEAR_FROM_NS)
        );
        assert!(!subset.strict.contains_key(&ComponentId::COMMIT_LOG_SIGNER));

        // The expected_registry must agree with the optional-seed
        // gating: no DM_MEMBERS / ONESHOT_MESSAGE entry, so the
        // semantic validator on the receiver side sees a symmetric
        // picture.
        assert!(
            !subset
                .expected_registry
                .contains_key(&ComponentId::DM_MEMBERS)
        );
        assert!(
            !subset
                .expected_registry
                .contains_key(&ComponentId::ONESHOT_MESSAGE)
        );

        assert!(subset.membership_sequence_ids.is_empty());
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn canonical_subset_dm_group_includes_dm_members() {
        let dm_members = crate::group_metadata::DmMembers {
            member_one_inbox_id: hex_inbox(0x22),
            member_two_inbox_id: hex_inbox(0x33),
        };
        let metadata = crate::group_metadata::GroupMetadata::new(
            xmtp_proto::types::ConversationType::Dm,
            hex_inbox(0x22),
            Some(dm_members.clone()),
            None,
        );
        let exts = build_test_extensions(
            default_gmm(),
            minimal_default_policy_set(),
            empty_membership(),
            metadata,
        );
        let subset = synthesize_canonical_subset_from_extensions(&exts).unwrap();

        let (_, dm_bytes) = subset
            .strict
            .get(&ComponentId::DM_MEMBERS)
            .expect("DM group must include DM_MEMBERS in strict");
        assert_eq!(*dm_bytes, encode_dm_members(&dm_members).unwrap());

        assert!(
            subset
                .expected_registry
                .contains_key(&ComponentId::DM_MEMBERS),
            "expected_registry must keep DM_MEMBERS for a DM group"
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn canonical_subset_oneshot_group_includes_oneshot() {
        use xmtp_proto::xmtp::mls::message_contents::OneshotMessage;
        // An empty `OneshotMessage` is enough to exercise the
        // presence-gated path — the wire bytes we assert on are the
        // prost encoding of whatever proto we feed in, not any
        // particular content shape.
        let oneshot = OneshotMessage { message_type: None };
        let metadata = crate::group_metadata::GroupMetadata::new(
            xmtp_proto::types::ConversationType::Group,
            hex_inbox(0x44),
            None,
            Some(oneshot.clone()),
        );
        let exts = build_test_extensions(
            default_gmm(),
            minimal_default_policy_set(),
            empty_membership(),
            metadata,
        );
        let subset = synthesize_canonical_subset_from_extensions(&exts).unwrap();

        let (_, oneshot_bytes) = subset
            .strict
            .get(&ComponentId::ONESHOT_MESSAGE)
            .expect("oneshot group must include ONESHOT_MESSAGE");
        assert_eq!(*oneshot_bytes, oneshot.encode_to_vec());

        assert!(
            subset
                .expected_registry
                .contains_key(&ComponentId::ONESHOT_MESSAGE)
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn canonical_subset_membership_sequence_ids() {
        let mut members = std::collections::HashMap::new();
        members.insert(hex_inbox(0x55), 7_u64);
        members.insert(hex_inbox(0x66), 42_u64);
        let membership = xmtp_proto::xmtp::mls::message_contents::GroupMembership {
            members,
            failed_installations: vec![],
        };

        let exts = build_test_extensions(
            default_gmm(),
            minimal_default_policy_set(),
            membership,
            plain_group_metadata(),
        );
        let subset = synthesize_canonical_subset_from_extensions(&exts).unwrap();

        assert_eq!(subset.membership_sequence_ids.len(), 2);
        assert_eq!(
            subset
                .membership_sequence_ids
                .get(&InboxId::from_bytes([0x55; 32]))
                .copied(),
            Some(7)
        );
        assert_eq!(
            subset
                .membership_sequence_ids
                .get(&InboxId::from_bytes([0x66; 32]))
                .copied(),
            Some(42)
        );
        // Empty legacy `failed_installations` → empty allow-set.
        assert!(subset.allowed_failed_installations.is_empty());
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn canonical_subset_collects_legacy_failed_installations_into_allow_set() {
        // Bound the sender's blast radius: every per-inbox
        // `failed_installations` byte-string the sender ships must come
        // from this set. Duplicates in the legacy list collapse — the
        // contract is set membership, not multiset.
        let installation_a = [0xAA_u8; 32];
        let installation_b = [0xBB_u8; 32];
        let membership = xmtp_proto::xmtp::mls::message_contents::GroupMembership {
            members: std::collections::HashMap::new(),
            failed_installations: vec![
                installation_a.to_vec(),
                installation_b.to_vec(),
                installation_a.to_vec(), // duplicate — collapses in BTreeSet
            ],
        };
        let exts = build_test_extensions(
            default_gmm(),
            minimal_default_policy_set(),
            membership,
            plain_group_metadata(),
        );
        let subset = synthesize_canonical_subset_from_extensions(&exts).unwrap();

        assert_eq!(subset.allowed_failed_installations.len(), 2);
        assert!(
            subset
                .allowed_failed_installations
                .contains(&installation_a)
        );
        assert!(
            subset
                .allowed_failed_installations
                .contains(&installation_b)
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn canonical_subset_rejects_non_32_byte_failed_installation() {
        // Anything other than a 32-byte Ed25519 installation key is
        // either corrupt legacy state or a wire-shape change — fail
        // loud rather than silently admit a bogus ID to the allow-set.
        let membership = xmtp_proto::xmtp::mls::message_contents::GroupMembership {
            members: std::collections::HashMap::new(),
            failed_installations: vec![vec![0xCC; 16]], // wrong length
        };
        let exts = build_test_extensions(
            default_gmm(),
            minimal_default_policy_set(),
            membership,
            plain_group_metadata(),
        );
        let err = synthesize_canonical_subset_from_extensions(&exts).unwrap_err();
        assert!(matches!(
            err,
            MigrationError::InvalidFailedInstallationLength(16)
        ));
    }

    /// Format one strict entry as a stable, diffable line.
    fn strict_lines(subset: &CanonicalBootstrapExpectation) -> Vec<String> {
        subset
            .strict
            .iter()
            .map(|(id, (op, bytes))| format!("{id} {op:?} {}", hex::encode(bytes)))
            .collect()
    }

    /// A fully-populated legacy fixture: every metadata attribute set,
    /// non-empty admin/super-admin lists. Fixed values only — the
    /// golden tests below pin the synthesis encoder's exact output.
    fn golden_gmm() -> crate::group_mutable_metadata::GroupMutableMetadata {
        use crate::group_mutable_metadata::MetadataField;
        let mut attributes = std::collections::HashMap::new();
        attributes.insert(
            MetadataField::GroupName.to_string(),
            "Golden Group".to_string(),
        );
        attributes.insert(
            MetadataField::Description.to_string(),
            "golden description".to_string(),
        );
        attributes.insert(
            MetadataField::GroupImageUrlSquare.to_string(),
            "https://example.com/golden.png".to_string(),
        );
        attributes.insert(
            MetadataField::AppData.to_string(),
            "golden-app-data".to_string(),
        );
        attributes.insert(
            MetadataField::MinimumSupportedProtocolVersion.to_string(),
            "1.11.0".to_string(),
        );
        attributes.insert(
            MetadataField::MessageDisappearFromNS.to_string(),
            "1000000000".to_string(),
        );
        attributes.insert(
            MetadataField::MessageDisappearInNS.to_string(),
            "2000000000".to_string(),
        );
        attributes.insert(
            MetadataField::CommitLogSigner.to_string(),
            hex::encode([0xAB; 32]),
        );
        crate::group_mutable_metadata::GroupMutableMetadata::new(
            attributes,
            vec![hex_inbox(0x22)],
            vec![hex_inbox(0x33)],
        )
    }

    /// GOLDEN VECTORS — the strict byte-compare surface of the
    /// bootstrap synthesis encoder, pinned byte-for-byte.
    ///
    /// Every fielded validator re-synthesizes these exact bytes from
    /// pre-flip group state and byte-compares them against incoming
    /// bootstrap commits (`bootstrap_validator.rs`), and groups migrate
    /// lazily — so this encoding must never change for inputs that old
    /// receivers can see. If this test fails, you have changed the
    /// bootstrap wire encoding: that is only shippable together with a
    /// `PROPOSALS_MIN_PROTOCOL_VERSION` bump (below-floor receivers
    /// then pause instead of byte-comparing — see the floor check in
    /// `validate_bootstrap_commit`), and the old expectation code must
    /// keep validating bootstraps produced by older senders. Do NOT
    /// simply re-pin the hex to make the test pass.
    #[xmtp_common::test(unwrap_try = true)]
    fn golden_bootstrap_synthesis_group() {
        let exts = build_test_extensions(
            golden_gmm(),
            minimal_default_policy_set(),
            empty_membership(),
            plain_group_metadata(),
        );
        let subset = synthesize_canonical_subset_from_extensions(&exts).unwrap();

        let expected: Vec<String> = [
            // SUPER_ADMIN_LIST (0x8001): TlsSet<InboxId>, one element
            // (vlen 0x22=34, then TLS-framed versioned InboxId).
            "0x8001 Update 2200003333333333333333333333333333333333333333333333333333333333333333",
            // ADMIN_LIST (0x8002): TlsSet<InboxId>, one element.
            "0x8002 Update 2200002222222222222222222222222222222222222222222222222222222222222222",
            // GROUP_NAME (0x8004): utf-8 passthrough.
            "0x8004 Update 476f6c64656e2047726f7570",
            // GROUP_DESCRIPTION (0x8005)
            "0x8005 Update 676f6c64656e206465736372697074696f6e",
            // GROUP_IMAGE_URL (0x8006)
            "0x8006 Update 68747470733a2f2f6578616d706c652e636f6d2f676f6c64656e2e706e67",
            // MESSAGE_DISAPPEAR_FROM_NS (0x8007): 8-byte BE i64 1_000_000_000.
            "0x8007 Update 000000003b9aca00",
            // MESSAGE_DISAPPEAR_IN_NS (0x8008): 8-byte BE i64 2_000_000_000.
            "0x8008 Update 0000000077359400",
            // APP_DATA (0x8009)
            "0x8009 Update 676f6c64656e2d6170702d64617461",
            // MIN_SUPPORTED_PROTOCOL_VERSION (0x800A): utf-8 "1.11.0".
            "0x800A Update 312e31312e30",
            // COMMIT_LOG_SIGNER (0x800B): raw 32 key bytes.
            "0x800B Update abababababababababababababababababababababababababababababababab",
            // CREATOR_INBOX_ID (0xBFFE): versioned InboxId (varint v0 + 32 bytes).
            "0xBFFE Update 001111111111111111111111111111111111111111111111111111111111111111",
            // CONVERSATION_TYPE (0xBFFF): BE i32, Group = 1.
            "0xBFFF Update 00000001",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        assert_eq!(
            strict_lines(&subset),
            expected,
            "bootstrap synthesis encoder output changed — see the doc comment \
             on this test before touching the pins"
        );
    }

    /// DM + oneshot variant of [`golden_bootstrap_synthesis_group`] —
    /// covers the optional strict seeds (DM_MEMBERS, ONESHOT_MESSAGE)
    /// the plain-group vector can't. Same freeze rules apply.
    #[xmtp_common::test(unwrap_try = true)]
    fn golden_bootstrap_synthesis_dm_with_oneshot() {
        let dm_members = crate::group_metadata::DmMembers {
            member_one_inbox_id: hex_inbox(0x22),
            member_two_inbox_id: hex_inbox(0x33),
        };
        // Empty proto: zero encoded bytes, but the presence-gated seed
        // still appears in strict — a stable pin that doesn't depend
        // on OneshotMessage's inner shape.
        let oneshot =
            xmtp_proto::xmtp::mls::message_contents::OneshotMessage { message_type: None };
        let metadata = crate::group_metadata::GroupMetadata::new(
            xmtp_proto::types::ConversationType::Dm,
            hex_inbox(0x22),
            Some(dm_members),
            Some(oneshot),
        );
        let exts = build_test_extensions(
            golden_gmm(),
            minimal_default_policy_set(),
            empty_membership(),
            metadata,
        );
        let subset = synthesize_canonical_subset_from_extensions(&exts).unwrap();

        let expected: Vec<String> = [
            "0x8001 Update 2200003333333333333333333333333333333333333333333333333333333333333333",
            "0x8002 Update 2200002222222222222222222222222222222222222222222222222222222222222222",
            "0x8004 Update 476f6c64656e2047726f7570",
            "0x8005 Update 676f6c64656e206465736372697074696f6e",
            "0x8006 Update 68747470733a2f2f6578616d706c652e636f6d2f676f6c64656e2e706e67",
            "0x8007 Update 000000003b9aca00",
            "0x8008 Update 0000000077359400",
            "0x8009 Update 676f6c64656e2d6170702d64617461",
            "0x800A Update 312e31312e30",
            "0x800B Update abababababababababababababababababababababababababababababababab",
            // ONESHOT_MESSAGE (0xBFFC): prost encoding of the empty
            // proto — zero bytes, seed present.
            "0xBFFC Update ",
            // DM_MEMBERS (0xBFFD): TlsSet<InboxId> of both members
            // (two-byte vlen 0x4044 = 68, elements in sorted order).
            "0xBFFD Update 40440000222222222222222222222222222222222222222222222222222222222222222200003333333333333333333333333333333333333333333333333333333333333333",
            // CREATOR_INBOX_ID (0xBFFE): versioned InboxId.
            "0xBFFE Update 002222222222222222222222222222222222222222222222222222222222222222",
            // CONVERSATION_TYPE (0xBFFF): BE i32, Dm = 2.
            "0xBFFF Update 00000002",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        assert_eq!(
            strict_lines(&subset),
            expected,
            "bootstrap synthesis encoder output changed — see the doc comment \
             on golden_bootstrap_synthesis_group before touching the pins"
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn canonical_subset_deterministic_across_calls() {
        // The validator byte-compares the sender's bootstrap commit
        // against this output on every receiver. Bit-identical output
        // from two calls on the same inputs is the entire point.
        let make = || {
            build_test_extensions(
                default_gmm(),
                minimal_default_policy_set(),
                empty_membership(),
                plain_group_metadata(),
            )
        };
        let a = synthesize_canonical_subset_from_extensions(&make()).unwrap();
        let b = synthesize_canonical_subset_from_extensions(&make()).unwrap();
        assert_eq!(a, b);
    }
}
