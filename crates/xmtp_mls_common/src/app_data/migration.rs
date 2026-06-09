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
    ComponentMetadata, ComponentPermissions, ComponentType,
    GroupMembership as GroupMembershipProto, MembershipPolicy as MembershipPolicyProto,
    MetadataPolicy as MetadataPolicyProto, PermissionsUpdatePolicy as PermissionsUpdatePolicyProto,
    PolicySet as PolicySetProto,
    membership_policy::{BasePolicy as MembershipBasePolicy, Kind as MembershipPolicyKind},
    metadata_policy::{Kind as MetadataPolicyKind, MetadataBasePolicy},
    permissions_update_policy::{Kind as PermissionsPolicyKind, PermissionsBasePolicy},
};

use crate::{
    app_data::{
        component_id::ComponentId,
        component_registry::{ComponentRegistry, ComponentRegistryError, new_component_metadata},
    },
    group_mutable_metadata::MetadataField,
    inbox_id::{InboxId, InboxIdError},
    tls_map::{TlsMap, TlsMapDelta, TlsMapMutation},
    tls_set::TlsSetDelta,
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

/// Produce a populated [`ComponentRegistry`] from the legacy
/// [`PolicySetProto`]. Deterministic: every honest peer synthesizes
/// bit-identical output from the same input.
///
/// Mapping summary:
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
///
/// The `(ComponentId, ComponentType)` pairs in this table must agree
/// with the static dispatch table at
/// [`super::registry_table::WELL_KNOWN`] — pinned by the unit test
/// `metadata_field_mapping_agrees_with_dispatch_table` below.
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

    // Immutable seeds. Route the DB-side `ConversationType` through
    // its `From<_> for ConversationTypeProto` impl before casting to
    // i32 — the two enums share variants today but are *separate*
    // types with their own discriminants. Direct `as i32` on the DB
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
    let policy_set = permissions_proto
        .policies
        .ok_or(MigrationError::MissingPolicyField("policies"))?;

    let legacy_metadata = crate::group_metadata::GroupMetadata::try_from(extensions)?;
    build_registry(
        &policy_set,
        legacy_metadata.dm_members.is_some(),
        legacy_metadata.oneshot_message.is_some(),
    )
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

/// Translate a legacy `GroupMutableMetadata` attribute string into the
/// AppData wire bytes for that component.
///
/// - `MESSAGE_DISAPPEAR_FROM_NS` / `MESSAGE_DISAPPEAR_IN_NS`: the legacy
///   attribute is a decimal-stringified `i64`; emit 8 big-endian bytes.
/// - String-typed attributes (group name/description/url/app data/min
///   version): emit the raw UTF-8 bytes unchanged.
fn encode_metadata_attribute_value(
    component_id: ComponentId,
    legacy_value: &str,
) -> Result<Vec<u8>, MigrationError> {
    match component_id {
        ComponentId::MESSAGE_DISAPPEAR_FROM_NS => {
            parse_disappearing_i64("messageDisappearFromNS", legacy_value)
        }
        ComponentId::MESSAGE_DISAPPEAR_IN_NS => {
            parse_disappearing_i64("messageDisappearInNS", legacy_value)
        }
        _ => Ok(legacy_value.as_bytes().to_vec()),
    }
}

fn parse_disappearing_i64(
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
fn encode_inbox_id_set(inbox_ids_hex: &[String]) -> Result<Vec<u8>, MigrationError> {
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
fn encode_dm_members(
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
                admitted_via_external_group_id: vec![],
            },
        );
        entries.insert(
            InboxId::from_bytes([0x02; 32]),
            GroupMembershipEntryV1 {
                sequence_id: 99,
                failed_installations: vec![],
                admitted_via_external_group_id: vec![],
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

    #[test]
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

    #[test]
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
                admitted_via_external_group_id: vec![],
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

    #[test]
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

    #[test]
    fn encode_inbox_id_set_rejects_bad_hex() {
        let err = encode_inbox_id_set(&["not-hex".to_string()]).unwrap_err();
        assert!(matches!(err, MigrationError::InvalidInboxId(_)));
    }

    #[test]
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

    #[test]
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

    #[test]
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

    #[test]
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

    #[test]
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
            xmtp_db::group::ConversationType::Group,
            hex_inbox(0x11),
            None,
            None,
        )
    }

    #[test]
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

    #[test]
    fn canonical_subset_dm_group_includes_dm_members() {
        let dm_members = crate::group_metadata::DmMembers {
            member_one_inbox_id: hex_inbox(0x22),
            member_two_inbox_id: hex_inbox(0x33),
        };
        let metadata = crate::group_metadata::GroupMetadata::new(
            xmtp_db::group::ConversationType::Dm,
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

    #[test]
    fn canonical_subset_oneshot_group_includes_oneshot() {
        use xmtp_proto::xmtp::mls::message_contents::OneshotMessage;
        // An empty `OneshotMessage` is enough to exercise the
        // presence-gated path — the wire bytes we assert on are the
        // prost encoding of whatever proto we feed in, not any
        // particular content shape.
        let oneshot = OneshotMessage { message_type: None };
        let metadata = crate::group_metadata::GroupMetadata::new(
            xmtp_db::group::ConversationType::Group,
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

    #[test]
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

    #[test]
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

    #[test]
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

    #[test]
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
