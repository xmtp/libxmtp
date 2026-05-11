//! Single source-of-truth for the per-`ComponentId` read/encode/apply logic.
//!
//! Centralizes everything the rest of the MLS pipeline needs to know about
//! a well-known `ComponentId`: its logical [`ComponentType`], where its
//! current bytes live (OpenMLS AppData dictionary vs. legacy group context
//! extensions), and how to encode/apply `AppDataUpdate` payloads.
//!
//! The module declaration is `pub` only so `ComponentSourceError` can
//! satisfy the `private_interfaces` lint on the public `GroupError` variant
//! it's embedded in. All helpers remain `pub(crate)`.
//!
//! ## Inbox-id encoding
//!
//! The legacy GMM extension stores inbox ids as 64-character hex strings.
//! Anything serialized through the new `AppDataUpdate` path uses the
//! versioned [`InboxId`] newtype instead — the legacy on-the-wire format
//! is left untouched for unmigrated groups.
//!
//! See [`xmtp_mls_common::inbox_id`] for the full wire-format contract;
//! the short version is `varint(version) || 32-byte payload`, with
//! version 0 producing a 33-byte encoding.

// `ComponentMutation`, `component_type`, and the standalone
// `expand_app_data_update_to_changes` entry point are scaffolding for
// the standalone proposal-by-reference flow (`IntentKind::ProposeAppDataUpdate`)
// described in XIP §1.5.2 / §3.4. They have unit-test coverage but no
// production caller yet — the inline path goes through
// `apply_app_data_update_payload` instead. `expect` (not `allow`) so the
// compiler trips this when standalone-propose wiring lands, and we
// either drop the attribute or trim whichever scaffolding the new path
// supersedes.
#![expect(dead_code)]

use openmls::{
    extensions::Extensions,
    group::{GroupContext, MlsGroup as OpenMlsGroup},
    messages::proposals::AppDataUpdateOperation,
};
use tls_codec::{Deserialize, Serialize};
use xmtp_mls_common::{
    app_data::{
        component_id::ComponentId,
        component_registry::ComponentRegistry,
        components::type_dispatch::{apply_update_payload_for_type, expand_to_changes_for_type},
        registry_table::lookup_component,
        typed::ComponentTypedError,
    },
    group_mutable_metadata::{
        GroupMutableMetadata, GroupMutableMetadataError, MetadataField,
        find_mutable_metadata_extension,
    },
    inbox_id::{InboxId, InboxIdError},
    tls_map::TlsMapError,
    tls_set::{TlsSet, TlsSetDelta, TlsSetError, TlsSetMutation},
};
use xmtp_proto::xmtp::mls::message_contents::ComponentType;

/// Errors surfaced by the component_source layer.
///
/// `pub` (rather than `pub(crate)`) because [`GroupError`] embeds it via
/// `#[from]` for the AppDataUpdate path; `pub(crate)` would trigger
/// `private_interfaces` warnings on the public `GroupError` variant.
///
/// [`GroupError`]: super::super::error::GroupError
#[derive(Debug, thiserror::Error)]
pub enum ComponentSourceError {
    /// The component is not in the well-known XMTP range that phase 1 handles.
    #[error("unknown component {0}")]
    UnknownComponent(ComponentId),

    /// The component is known but its wiring hasn't been built yet.
    #[error("component {0} wiring is not yet implemented")]
    NotImplemented(ComponentId),

    /// An `AppDataUpdate::Update` write was attempted against an immutable
    /// component. Insert-once writes should be expressed as `Insert`, not
    /// caught here.
    #[error("component {0} is immutable and cannot be updated via AppDataUpdate")]
    ImmutableUpdate(ComponentId),

    /// The supplied [`ComponentMutation`] does not match the component type
    /// of the component it targets (e.g. a `Bytes` mutation against
    /// `ADMIN_LIST`).
    #[error("mutation shape does not match component {0}")]
    MismatchedMutation(ComponentId),

    /// Failed to convert an inbox id string or byte slice into an
    /// [`InboxId`]. Wraps [`InboxIdError`] — callers that need to
    /// distinguish "not hex" from "wrong length" can match the inner
    /// variant.
    #[error("invalid inbox id: {0}")]
    InvalidInboxId(#[from] InboxIdError),

    /// A wire-format violation on a component value: the bytes stored in
    /// the AppData dictionary for a known component don't decode under the
    /// expected encoding (e.g. non-UTF-8 bytes for a `Bytes`-typed
    /// metadata attribute, malformed `TlsSet` for a collection component).
    #[error("malformed value for component {component_id}: {reason}")]
    MalformedComponentValue {
        /// The component whose stored bytes failed to decode.
        component_id: ComponentId,
        /// Human-readable reason — surface to logs, not user-facing.
        reason: String,
    },

    /// A `MetadataUpdate` intent referenced a metadata field name that has
    /// no corresponding `ComponentId`. Most commonly fires when a future
    /// metadata field is added to one of the senders without also being
    /// added to [`metadata_field_to_component_id`].
    #[error("unknown metadata field name: {0}")]
    UnknownMetadataField(String),

    /// Failed to read, decode, or encode the legacy group mutable metadata
    /// extension while servicing a component-source request.
    #[error(transparent)]
    GroupMutableMetadata(#[from] GroupMutableMetadataError),

    /// A TLS-codec operation on a delta or stored collection value failed.
    #[error("tls codec error: {0}")]
    TlsCodec(#[from] tls_codec::Error),

    /// A `TlsSet::apply_delta` call failed while synthesizing the new full
    /// value of a collection component from an incoming delta.
    #[error("tls set apply error: {0}")]
    TlsSetApply(#[from] TlsSetError),

    /// A `TlsMap::apply_delta` call failed while synthesizing the new full
    /// value of a map component from an incoming delta.
    #[error("tls map apply error: {0}")]
    TlsMapApply(#[from] TlsMapError),
}

impl ComponentSourceError {
    /// Best-effort `ComponentId` extraction for the variants that carry
    /// one — so error-mapping shims can preserve structured context
    /// across the crate boundary into
    /// [`GroupMutableMetadataError::MalformedComponent`] without
    /// stringifying.
    pub(crate) fn component_id(&self) -> Option<ComponentId> {
        match self {
            Self::UnknownComponent(id)
            | Self::NotImplemented(id)
            | Self::ImmutableUpdate(id)
            | Self::MismatchedMutation(id)
            | Self::MalformedComponentValue {
                component_id: id, ..
            } => Some(*id),
            _ => None,
        }
    }
}

impl From<ComponentTypedError> for ComponentSourceError {
    /// Surface trait-layer errors at the dispatch boundary. The
    /// dispatch layer adds `UnknownComponent` / `NotImplemented` /
    /// `UnknownMetadataField` / `GroupMutableMetadata` for things the
    /// trait can't see; the variants below are the trait's domain
    /// and round-trip 1:1.
    fn from(err: ComponentTypedError) -> Self {
        match err {
            ComponentTypedError::ImmutableUpdate(id) => Self::ImmutableUpdate(id),
            ComponentTypedError::MismatchedMutation(id) => Self::MismatchedMutation(id),
            ComponentTypedError::MalformedValue {
                component_id,
                reason,
            } => Self::MalformedComponentValue {
                component_id,
                reason,
            },
            ComponentTypedError::InvalidInboxId(e) => Self::InvalidInboxId(e),
            ComponentTypedError::TlsCodec(e) => Self::TlsCodec(e),
            ComponentTypedError::TlsSetApply(e) => Self::TlsSetApply(e),
            ComponentTypedError::TlsMapApply(e) => Self::TlsMapApply(e),
            ComponentTypedError::UnspecifiedType(id) => Self::MalformedComponentValue {
                component_id: id,
                reason: "registered ComponentType is Unspecified".to_string(),
            },
        }
    }
}

impl From<ComponentSourceError> for GroupMutableMetadataError {
    /// Preserve structure where possible. If the source already wraps a
    /// `GroupMutableMetadataError` (e.g. `MissingExtension` raised by the
    /// legacy `TryFrom<&OpenMlsGroup>` path on an unmigrated group),
    /// unwrap and return that inner variant verbatim so callers can
    /// match on `MissingExtension` / `MissingMetadataField` / etc.
    ///
    /// For every other variant, surface as `MalformedComponent` and
    /// preserve the offending `component_id` when it's available so
    /// downstream consumers (bindings, error-mapping) can match
    /// structurally on it. Variants without one surface as
    /// `component_id: None`; the display string stays the
    /// authoritative diagnostic.
    fn from(err: ComponentSourceError) -> Self {
        if let ComponentSourceError::GroupMutableMetadata(inner) = err {
            return inner;
        }
        let component_id = err.component_id();
        GroupMutableMetadataError::MalformedComponent {
            component_id,
            reason: err.to_string(),
        }
    }
}

/// Describes a single, atomic mutation that a per-field intent handler wants
/// to apply to a component. The encoder picks the wire shape (single-element
/// [`TlsSetDelta`] for collections, passthrough for bytes components).
///
/// The wire format supports batching (`TlsSetDelta.mutations` is a
/// `Vec<TlsSetMutation<K>>`), but this enum intentionally models a single
/// atomic mutation per variant — admin-list updates today arrive as
/// single-action intents (`UpdateAdminListIntentData` carries one inbox
/// id and one action), and coalescing happens at the commit layer via
/// [`super::accumulate_app_data_updates`]. The migration PR that wires
/// admin-list paths through `AppDataUpdate` should reshape this into
/// batched variants (e.g. `InboxIdSetDelta { component_id, mutations }`)
/// so a single proposal can carry multiple set mutations.
#[derive(Debug, Clone)]
pub(crate) enum ComponentMutation<'a> {
    /// A whole-value replacement for a `Bytes`-typed component.
    Bytes {
        component_id: ComponentId,
        new_value: &'a [u8],
    },
    /// Add a single inbox id to the admin list.
    AdminListAdd { inbox_id: &'a str },
    /// Remove a single inbox id from the admin list.
    AdminListRemove { inbox_id: &'a str },
    /// Add a single inbox id to the super-admin list.
    SuperAdminListAdd { inbox_id: &'a str },
    /// Remove a single inbox id from the super-admin list.
    SuperAdminListRemove { inbox_id: &'a str },
}

impl ComponentMutation<'_> {
    /// The `ComponentId` that this mutation targets.
    pub(crate) fn component_id(&self) -> ComponentId {
        match self {
            Self::Bytes { component_id, .. } => *component_id,
            Self::AdminListAdd { .. } | Self::AdminListRemove { .. } => ComponentId::ADMIN_LIST,
            Self::SuperAdminListAdd { .. } | Self::SuperAdminListRemove { .. } => {
                ComponentId::SUPER_ADMIN_LIST
            }
        }
    }
}

/// Hardcoded logical type of a well-known component. Returns `None` for
/// app-range components (`0xC000-0xFEFF`) and for any well-known id that
/// phase 1 has not yet mapped.
pub(crate) fn component_type(id: ComponentId) -> Option<ComponentType> {
    match id {
        // Hardcoded registry / list components. ComponentRegistry itself is a
        // TlsMap, but permissions are enforced in code — it never flows
        // through this module.
        ComponentId::COMPONENT_REGISTRY => Some(ComponentType::TlsMapBytesBytes),
        ComponentId::SUPER_ADMIN_LIST => Some(ComponentType::TlsSetInboxId),
        ComponentId::ADMIN_LIST => Some(ComponentType::TlsSetInboxId),

        // GroupMembership — TlsMap<InboxId, bytes>
        ComponentId::GROUP_MEMBERSHIP => Some(ComponentType::TlsMapInboxIdBytes),

        // GroupMutableMetadata-backed string components.
        ComponentId::GROUP_NAME
        | ComponentId::GROUP_DESCRIPTION
        | ComponentId::GROUP_IMAGE_URL
        | ComponentId::APP_DATA
        | ComponentId::MIN_SUPPORTED_PROTOCOL_VERSION => Some(ComponentType::String),

        // GroupMutableMetadata-backed bytes components.
        ComponentId::MESSAGE_DISAPPEAR_FROM_NS
        | ComponentId::MESSAGE_DISAPPEAR_IN_NS
        | ComponentId::COMMIT_LOG_SIGNER => Some(ComponentType::Bytes),

        // Immutable metadata (not flowable through AppDataUpdate writes in
        // phase 1, but we still advertise the type for completeness).
        ComponentId::CONVERSATION_TYPE
        | ComponentId::CREATOR_INBOX_ID
        | ComponentId::ONESHOT_MESSAGE => Some(ComponentType::Bytes),
        ComponentId::DM_MEMBERS => Some(ComponentType::TlsSetInboxId),

        _ => None,
    }
}

/// Single source of truth for the `MetadataField` ↔ `ComponentId` bijection
/// over the Bytes-typed mutable-metadata family. Both lookup helpers below
/// and `merge_app_data_into_mutable_metadata` derive from this table.
const METADATA_FIELD_COMPONENT_MAP: &[(MetadataField, ComponentId)] = &[
    (MetadataField::GroupName, ComponentId::GROUP_NAME),
    (MetadataField::Description, ComponentId::GROUP_DESCRIPTION),
    (
        MetadataField::GroupImageUrlSquare,
        ComponentId::GROUP_IMAGE_URL,
    ),
    (
        MetadataField::MessageDisappearFromNS,
        ComponentId::MESSAGE_DISAPPEAR_FROM_NS,
    ),
    (
        MetadataField::MessageDisappearInNS,
        ComponentId::MESSAGE_DISAPPEAR_IN_NS,
    ),
    (
        MetadataField::MinimumSupportedProtocolVersion,
        ComponentId::MIN_SUPPORTED_PROTOCOL_VERSION,
    ),
    (
        MetadataField::CommitLogSigner,
        ComponentId::COMMIT_LOG_SIGNER,
    ),
    (MetadataField::AppData, ComponentId::APP_DATA),
];

/// Map a [`MetadataField`] string to its corresponding `ComponentId`.
///
/// Returns `None` for unknown field names so this can also be called with a
/// raw string coming from a legacy intent payload.
pub(crate) fn metadata_field_to_component_id(field_name: &str) -> Option<ComponentId> {
    METADATA_FIELD_COMPONENT_MAP
        .iter()
        .find(|(field, _)| field.as_str() == field_name)
        .map(|(_, id)| *id)
}

/// Map a `ComponentId` back to the `MetadataField` attribute name that stores
/// it in the legacy `GroupMutableMetadata` extension.
///
/// Returns `None` for component ids that are not backed by a
/// `GroupMutableMetadata` attribute (e.g. `ADMIN_LIST`, `GROUP_MEMBERSHIP`,
/// or anything outside the mutable metadata family).
pub(crate) fn component_id_to_metadata_field(id: ComponentId) -> Option<MetadataField> {
    METADATA_FIELD_COMPONENT_MAP
        .iter()
        .find(|(_, component_id)| *component_id == id)
        .map(|(field, _)| *field)
}

/// Read the component's current bytes from whichever storage the group's
/// capability flag indicates: the OpenMLS AppData dictionary when
/// `proposals_enabled` is on, otherwise the legacy group context
/// extensions (translated into the new app-data wire format on the fly).
pub(crate) fn read_component_bytes(
    id: ComponentId,
    mls_group: &OpenMlsGroup,
    proposals_enabled: bool,
) -> Result<Option<Vec<u8>>, ComponentSourceError> {
    if proposals_enabled {
        Ok(read_from_app_data_dict(id, mls_group))
    } else {
        read_from_legacy(id, mls_group.extensions())
    }
}

/// Look up the component's bytes in the OpenMLS AppData dictionary.
///
/// `pub(crate)` so the commit validator (`validated_commit.rs`) can
/// pull the pre-commit stored bytes for a component and thread them
/// into [`expand_app_data_update_to_changes`] as `old_value` — the
/// validator uses that to resolve `RemoveByHash` mutations back to
/// their underlying inbox id. The parent `app_data` module also uses
/// it from `process_message_with_app_data`, `stage_app_data_propose_and_commit`,
/// and `pending_app_data_updates`.
pub(crate) fn read_from_app_data_dict(
    id: ComponentId,
    mls_group: &OpenMlsGroup,
) -> Option<Vec<u8>> {
    let openmls_id: openmls::component::ComponentId = id.as_u16();
    mls_group
        .extensions()
        .app_data_dictionary()
        .and_then(|ext| ext.dictionary().get(&openmls_id))
        .map(|bytes| bytes.to_vec())
}

/// Look up the component's bytes in the legacy group-context extensions and
/// translate them into the new app-data wire format.
///
/// For `GroupMutableMetadata`-backed bytes components this returns the
/// attribute's UTF-8 bytes. For `ADMIN_LIST` / `SUPER_ADMIN_LIST` it
/// re-encodes the legacy `Vec<String>` of hex inbox ids as a
/// `TlsSet<InboxId>`. For `GROUP_MEMBERSHIP` this is currently a stub
/// returning [`ComponentSourceError::NotImplemented`] — see §3 of the plan.
fn read_from_legacy(
    id: ComponentId,
    extensions: &Extensions<GroupContext>,
) -> Result<Option<Vec<u8>>, ComponentSourceError> {
    // Mutable-metadata-backed bytes components: pull the attribute out of
    // the GMM extension. Missing extension → None; missing attribute → None.
    if let Some(field) = component_id_to_metadata_field(id) {
        let gmm = match find_mutable_metadata_extension(extensions) {
            Some(bytes) => GroupMutableMetadata::try_from(bytes)?,
            None => return Ok(None),
        };
        return Ok(gmm
            .attributes
            .get(field.as_str())
            .map(|s| s.as_bytes().to_vec()));
    }

    match id {
        ComponentId::ADMIN_LIST => {
            let gmm = match find_mutable_metadata_extension(extensions) {
                Some(bytes) => GroupMutableMetadata::try_from(bytes)?,
                None => return Ok(None),
            };
            Ok(Some(encode_inbox_id_set(&gmm.admin_list)?))
        }
        ComponentId::SUPER_ADMIN_LIST => {
            let gmm = match find_mutable_metadata_extension(extensions) {
                Some(bytes) => GroupMutableMetadata::try_from(bytes)?,
                None => return Ok(None),
            };
            Ok(Some(encode_inbox_id_set(&gmm.super_admin_list)?))
        }
        ComponentId::GROUP_MEMBERSHIP => Err(ComponentSourceError::NotImplemented(id)),
        _ => Err(ComponentSourceError::UnknownComponent(id)),
    }
}

/// Encode a [`ComponentMutation`] into the bytes that go inside an
/// `AppDataUpdateOperation::Update(bytes)` payload on the wire.
///
/// - `Bytes` components pass through verbatim.
/// - `AdminList*` / `SuperAdminList*` produce a single-element
///   [`TlsSetDelta`] keyed on an [`InboxId`].
pub(crate) fn encode_app_data_update_payload(
    mutation: &ComponentMutation<'_>,
) -> Result<Vec<u8>, ComponentSourceError> {
    match mutation {
        ComponentMutation::Bytes {
            component_id,
            new_value,
        } => {
            // Phase-1 bytes components only cover the GMM-attribute family.
            if component_id_to_metadata_field(*component_id).is_none() {
                return Err(ComponentSourceError::MismatchedMutation(*component_id));
            }
            Ok(new_value.to_vec())
        }
        ComponentMutation::AdminListAdd { inbox_id }
        | ComponentMutation::SuperAdminListAdd { inbox_id } => {
            let key = inbox_id_str_to_bytes(inbox_id)?;
            encode_inbox_id_set_delta(TlsSetMutation::Insert(key))
        }
        ComponentMutation::AdminListRemove { inbox_id }
        | ComponentMutation::SuperAdminListRemove { inbox_id } => {
            let key = inbox_id_str_to_bytes(inbox_id)?;
            encode_inbox_id_set_delta(TlsSetMutation::Remove(key))
        }
    }
}

// `ExpandedComponentChange` lives in `xmtp_mls_common::app_data::typed`
// so the `Component` trait there can return it. Re-exported here so
// in-crate callers can construct the change list without pulling the
// xmtp_mls_common path in directly.
pub(crate) use xmtp_mls_common::app_data::typed::ExpandedComponentChange;

/// Expand an `AppDataUpdate` proposal payload into the per-element changes
/// that should be checked against the component registry.
///
/// - `Bytes` components: returns a single `Update` change with the new
///   payload bytes.
/// - Collection components (`ADMIN_LIST` / `SUPER_ADMIN_LIST`): parses the
///   payload as a `TlsSetDelta<InboxId>` and emits one entry per
///   mutation, with `op = Insert` for `Insert`, `op = Delete` for
///   `Remove` / `RemoveByHash`.
/// - `AppDataUpdateOperation::Remove` (any component): a single
///   `Delete` entry with no value.
///
/// `old_value` is the component's pre-commit stored bytes (from the
/// AppData dictionary). It's only consulted for `RemoveByHash`
/// resolution on collection components: given the prior `TlsSet<InboxId>`,
/// we build a `hash → InboxId` index and resolve each `RemoveByHash` back
/// to the concrete key being removed so the validator sees the inbox id
/// the peer is targeting. If the hash doesn't match any prior key (or
/// `old_value` is `None`), the expansion surfaces `value: None` and the
/// subsequent CRDT apply step surfaces the real error.
///
/// Used on the receiver side to feed `validate_component_write` for each
/// distinct change inside a single `AppDataUpdate` proposal.
///
/// The steady-state validator dispatches through `lookup_component`
/// directly so it can also call `Component::validate_invariant`
/// without a second binary search. This wrapper is retained for
/// callers that don't need the invariant hook.
pub(crate) fn expand_app_data_update_to_changes(
    component_id: ComponentId,
    operation: &AppDataUpdateOperation,
    old_value: Option<&[u8]>,
    registry: &ComponentRegistry,
) -> Result<Vec<ExpandedComponentChange>, ComponentSourceError> {
    if let Some(component) = lookup_component(component_id) {
        return component
            .expand_to_changes(operation, old_value)
            .map_err(Into::into);
    }

    // No per-id [`Component`] impl on this client. Two type-resolution
    // sources, tried in order:
    //
    // 1. In-code [`component_type`] mapping — covers well-known XMTP
    //    ids whose type is known to this release but which have no
    //    typed decoder (e.g. the immutable seeds CREATOR_INBOX_ID,
    //    ONESHOT_MESSAGE — handled by bootstrap byte-compare, not by a
    //    `Component` impl).
    // 2. On-dict [`ComponentRegistry`] entry — covers components a
    //    *newer* release ships that this client has never heard of;
    //    the registry's `component_type` tag is the type oracle.
    //
    // Either way, the closed type universe (6 variants) means every
    // shape — including `TlsSet` / `TlsMap` deltas — surfaces a proper
    // per-element change list to the validator. Old and new clients
    // converge on the same dict state for the same wire bytes.
    let ty = component_type(component_id)
        .map_or_else(|| registered_component_type(component_id, registry), Ok)?;
    expand_to_changes_for_type(component_id, ty, operation, old_value).map_err(Into::into)
}

/// Decode an incoming `AppDataUpdateOperation::Update(bytes)` payload
/// and produce the new full bytes of the component, given the prior
/// stored bytes (if any). `Update`-only — `Remove` carries no payload
/// and is handled directly by the caller.
///
/// Immutable components are rejected with
/// [`ComponentSourceError::ImmutableUpdate`] **only when a prior
/// value already exists** — the bootstrap commit is the canonical
/// first-insert path for immutable seeds, so this layer must allow
/// an `Update` whose `old_value` is `None`. The bootstrap validator
/// catches malicious initial values upstream via byte-compare.
pub(crate) fn apply_app_data_update_payload(
    id: ComponentId,
    payload: &[u8],
    old_value: Option<&[u8]>,
    registry: &ComponentRegistry,
) -> Result<Vec<u8>, ComponentSourceError> {
    // Immutability gate. Reject only on overwrite — a fresh insert
    // (no prior value) is the bootstrap commit's first write of an
    // immutable seed and must succeed for honest receivers to reach
    // the migrated state. Steady-state immutables always have a prior
    // (inserted at bootstrap), so a Byzantine peer trying to mutate
    // them post-bootstrap still hits this branch and gets rejected.
    if id.is_immutable() && old_value.is_some() {
        return Err(ComponentSourceError::ImmutableUpdate(id));
    }

    // Per-id `Component` impl on this client — handles all 13 well-
    // known mutable components with a typed decoder.
    if let Some(component) = lookup_component(id) {
        return component
            .apply_update_payload(payload, old_value)
            .map_err(Into::into);
    }

    // Two type-resolution sources for components without a per-id
    // impl, tried in order:
    //
    // 1. In-code [`component_type`] mapping — covers well-known XMTP
    //    ids whose type is known but which have no typed decoder
    //    (immutable seeds like CREATOR_INBOX_ID — bootstrap-only
    //    first-write path).
    // 2. On-dict [`ComponentRegistry`] entry — covers components a
    //    *newer* release ships that this client has never heard of;
    //    the registry's `component_type` tag is the type oracle.
    let ty = component_type(id).map_or_else(|| registered_component_type(id, registry), Ok)?;
    apply_update_payload_for_type(id, ty, payload, old_value).map_err(Into::into)
}

/// Look up the [`ComponentType`] registered for a component id in the
/// on-dict [`ComponentRegistry`]. Returns
/// [`ComponentSourceError::UnknownComponent`] when no registry entry
/// exists — the deny-by-default rule that keeps unrecognized payloads
/// from being applied opaquely.
fn registered_component_type(
    id: ComponentId,
    registry: &ComponentRegistry,
) -> Result<ComponentType, ComponentSourceError> {
    let meta = registry
        .get(&id)
        .map_err(|e| ComponentSourceError::MalformedComponentValue {
            component_id: id,
            reason: format!("registry lookup: {e}"),
        })?
        .ok_or(ComponentSourceError::UnknownComponent(id))?;
    ComponentType::try_from(meta.component_type).map_err(|_| {
        ComponentSourceError::MalformedComponentValue {
            component_id: id,
            reason: format!(
                "registry entry has unknown component_type tag {}",
                meta.component_type
            ),
        }
    })
}

/// Overlay AppData-dict component values onto a base [`GroupMutableMetadata`]
/// read from the legacy extension. On migrated groups the dict is
/// authoritative; for unmigrated components the legacy GMM stays as the
/// fallback, so callers always get a complete view.
///
/// Gated on [`super::is_migrated_group`] (defense-in-depth) so a stray
/// dict entry on a pre-bootstrap group can't shadow legacy GMM.
///
/// Wire formats (must match what the sender emits via
/// [`encode_app_data_update_payload`] / [`apply_app_data_update_payload`]):
/// - Bytes components: raw UTF-8 string bytes.
/// - `ADMIN_LIST` / `SUPER_ADMIN_LIST`: TLS-serialized `TlsSet<InboxId>`,
///   each id hex-encoded back to string form.
///
/// ## Independence from `COMPONENT_REGISTRY` parseability
///
/// This function reads metadata field entries directly from the dict and
/// **never** loads or validates the `COMPONENT_REGISTRY` payload — it
/// only uses [`super::is_migrated_extensions`] (key-existence check) as
/// the gate. So a malformed `COMPONENT_REGISTRY` blob does NOT cause
/// metadata reads to drop authoritative data: as long as the individual
/// metadata field bytes (`GROUP_NAME`, `ADMIN_LIST`, …) decode
/// correctly, they round-trip into the returned GMM. Registry corruption
/// is surfaced loudly on the *write* paths instead — the sender gate in
/// `mls_sync.rs` and the commit validator in `validated_commit.rs` both
/// call [`super::load_component_registry`] and propagate decode errors
/// — so a corrupt registry blocks state changes without making readable
/// data unreachable. See
/// `merge_with_malformed_registry_returns_valid_field` for the test
/// that pins this invariant.
pub(crate) fn merge_app_data_into_mutable_metadata(
    base: &mut GroupMutableMetadata,
    mls_group: &OpenMlsGroup,
) -> Result<(), ComponentSourceError> {
    merge_app_data_into_mutable_metadata_from_extensions(base, mls_group.extensions())
}

/// Capability-aware [`GroupMutableMetadata`] extractor.
///
/// On migrated groups the legacy `GroupMutableMetadata` group context
/// extension is stripped by the bootstrap commit, so the static
/// [`xmtp_mls_common::group_mutable_metadata::extract_legacy_group_mutable_metadata`]
/// returns `MissingExtension` and any caller that swallows the error
/// with `.ok()` silently defaults every metadata field (notably:
/// disappearing-message settings and `MinimumSupportedProtocolVersion`
/// — the latter is what gates the XIP §3 pause-on-version-bump flow).
///
/// This helper returns the same `GroupMutableMetadata` shape but reads
/// from the right source per migration state:
///
/// - **Migrated** ([`super::is_migrated_group`] returns `true`): starts
///   from an empty composite and overlays every field from the AppData
///   dictionary via [`merge_app_data_into_mutable_metadata`].
/// - **Unmigrated**: parses the legacy GMM extension via
///   `GroupMutableMetadata::try_from(&OpenMlsGroup)`, matching the
///   legacy static helper byte-for-byte.
pub(crate) fn extract_group_mutable_metadata_capability_aware(
    mls_group: &OpenMlsGroup,
) -> Result<GroupMutableMetadata, ComponentSourceError> {
    if super::is_migrated_group(mls_group) {
        let mut base =
            GroupMutableMetadata::new(std::collections::HashMap::new(), Vec::new(), Vec::new());
        merge_app_data_into_mutable_metadata(&mut base, mls_group)?;
        Ok(base)
    } else {
        Ok(GroupMutableMetadata::try_from(mls_group)?)
    }
}

/// Extensions-only variant of [`merge_app_data_into_mutable_metadata`].
/// Mirrors the [`super::is_migrated_group`] / [`super::is_migrated_extensions`]
/// and [`super::load_component_registry`] /
/// [`super::load_component_registry_from_extensions`] splits so unit
/// tests can pin the merge contract without materializing an
/// `OpenMlsGroup`.
pub(crate) fn merge_app_data_into_mutable_metadata_from_extensions(
    base: &mut GroupMutableMetadata,
    extensions: &openmls::extensions::Extensions<openmls::group::GroupContext>,
) -> Result<(), ComponentSourceError> {
    if !super::is_migrated_extensions(extensions) {
        return Ok(());
    }
    let Some(ext) = extensions.app_data_dictionary() else {
        return Ok(());
    };
    let dict = ext.dictionary();

    for (field, id) in METADATA_FIELD_COMPONENT_MAP {
        if let Some(bytes) = dict.get(&id.as_u16()) {
            // Each typed `Component`'s wire shape decides how the dict
            // bytes round-trip back into the legacy
            // `GroupMutableMetadata.attributes` string map:
            //
            // - `MESSAGE_DISAPPEAR_*` are 8-byte BE `i64` on the wire;
            //   format as a base-10 string for the legacy reader.
            // - `COMMIT_LOG_SIGNER` is the raw 32-byte private key on
            //   the wire; hex-encode for the legacy reader.
            // - All other metadata-attribute components are UTF-8.
            let legacy_value = match *id {
                ComponentId::MESSAGE_DISAPPEAR_FROM_NS | ComponentId::MESSAGE_DISAPPEAR_IN_NS => {
                    let arr: [u8; 8] = bytes.try_into().map_err(|_| {
                        ComponentSourceError::MalformedComponentValue {
                            component_id: *id,
                            reason: format!("expected 8 bytes (BE i64), got {}", bytes.len()),
                        }
                    })?;
                    i64::from_be_bytes(arr).to_string()
                }
                ComponentId::COMMIT_LOG_SIGNER => hex::encode(bytes),
                _ => std::str::from_utf8(bytes)
                    .map_err(|e| ComponentSourceError::MalformedComponentValue {
                        component_id: *id,
                        reason: format!("non-UTF-8 bytes: {e}"),
                    })?
                    .to_string(),
            };
            base.attributes
                .insert(field.as_str().to_string(), legacy_value);
        }
    }

    // ADMIN_LIST / SUPER_ADMIN_LIST overlay: on migrated groups the
    // dict is authoritative; decode the `TlsSet<InboxId>` and
    // hex-encode each id back to string form for the base GMM.
    for (component_id, list) in [
        (ComponentId::ADMIN_LIST, &mut base.admin_list),
        (ComponentId::SUPER_ADMIN_LIST, &mut base.super_admin_list),
    ] {
        if let Some(bytes) = dict.get(&component_id.as_u16()) {
            let set = TlsSet::<InboxId>::tls_deserialize_exact(bytes).map_err(|e| {
                ComponentSourceError::MalformedComponentValue {
                    component_id,
                    reason: format!("invalid TlsSet<InboxId>: {e}"),
                }
            })?;
            *list = set.iter().map(|id| id.to_hex()).collect();
        }
    }
    Ok(())
}

// ============================================================================
// Inbox-id encoding helpers
// ============================================================================
//
// Inbox ids are SHA-256 hashes (see `xmtp_id::associations::member::inbox_id`).
// Their canonical string form is a 64-character hex string. Anything we put
// on the wire through the new `AppDataUpdate` path uses the
// versioned `InboxId` newtype instead — see the module-level docs for
// the rationale and `xmtp_mls_common::inbox_id` for the full contract.

/// Decode a hex-string inbox id into an [`InboxId`].
///
/// Returns [`ComponentSourceError::InvalidInboxId`] wrapping either
/// [`InboxIdError::InvalidHex`] (input wasn't hex) or
/// [`InboxIdError::InvalidLength`] (wrong byte length after decoding).
/// Callers that need to distinguish the failure modes can match the
/// inner variant.
pub(crate) fn inbox_id_str_to_bytes(inbox_id: &str) -> Result<InboxId, ComponentSourceError> {
    InboxId::from_hex(inbox_id).map_err(Into::into)
}

/// Read the super-admin list from the AppData dictionary on a migrated
/// group. Returns `Ok(None)` on unmigrated groups (or migrated groups
/// that happen not to have written `SUPER_ADMIN_LIST` yet).
///
/// Gated on [`super::is_migrated_group`] for the same reason as
/// [`merge_app_data_into_mutable_metadata`] — keep stray dict entries
/// from shadowing the authoritative legacy path pre-bootstrap.
pub(crate) fn read_super_admin_list_from_dict(
    mls_group: &OpenMlsGroup,
) -> Result<Option<Vec<String>>, ComponentSourceError> {
    read_super_admin_list_from_extensions(mls_group.extensions())
}

/// Extensions-only variant of [`read_super_admin_list_from_dict`]. Use
/// the shim above when an `OpenMlsGroup` is at hand; this form is
/// available primarily for unit testing and for commit-validation
/// paths that only carry an `Extensions` reference.
pub(crate) fn read_super_admin_list_from_extensions(
    extensions: &Extensions<GroupContext>,
) -> Result<Option<Vec<String>>, ComponentSourceError> {
    if !super::is_migrated_extensions(extensions) {
        return Ok(None);
    }
    let Some(ext) = extensions.app_data_dictionary() else {
        return Ok(None);
    };
    let Some(bytes) = ext
        .dictionary()
        .get(&ComponentId::SUPER_ADMIN_LIST.as_u16())
    else {
        return Ok(None);
    };
    let set = TlsSet::<InboxId>::tls_deserialize_exact(bytes).map_err(|e| {
        ComponentSourceError::MalformedComponentValue {
            component_id: ComponentId::SUPER_ADMIN_LIST,
            reason: format!("invalid TlsSet<InboxId>: {e}"),
        }
    })?;
    Ok(Some(set.iter().map(|id| id.to_hex()).collect()))
}

/// Synthesize a [`GroupMetadata`] from the AppData dictionary on a
/// migrated group. Returns `Ok(None)` if the critical immutable seeds
/// aren't present (unmigrated group).
///
/// Encoding mirrors the sender-side synthesis in
/// [`xmtp_mls_common::app_data::migration::synthesize_canonical_subset_for_validation`]:
/// - `CONVERSATION_TYPE`: 4 big-endian bytes of `ConversationType as i32`
///   (see `encode_conversation_type` there).
/// - `CREATOR_INBOX_ID`: the versioned `InboxId` TLS wire form
///   (`varint(version) || 32-byte payload`) — the same shape every
///   other inbox-id-bearing component on the new path uses. Reader
///   hex-encodes the decoded id back into the legacy
///   `GroupMetadata::creator_inbox_id: String` slot.
/// - `DM_MEMBERS`: `TlsSet<InboxId>` with exactly two elements —
///   matches the declared `ComponentType::TlsSetInboxId` and the
///   sender's `encode_dm_members`. The writer rejects self-DMs
///   (identical slots) up front; readers that see a 1-element set
///   surface `MalformedComponentValue`.
/// - `ONESHOT_MESSAGE`: prost-encoded `OneshotMessage`.
pub(crate) fn read_group_metadata_from_dict(
    mls_group: &OpenMlsGroup,
) -> Result<Option<GroupMetadataReturn>, ComponentSourceError> {
    read_group_metadata_from_extensions(mls_group.extensions())
}

/// Extensions-only variant of [`read_group_metadata_from_dict`]. Same
/// rationale for the split as [`read_super_admin_list_from_extensions`].
pub(crate) fn read_group_metadata_from_extensions(
    extensions: &Extensions<GroupContext>,
) -> Result<Option<GroupMetadataReturn>, ComponentSourceError> {
    use prost::Message;
    use xmtp_proto::xmtp::mls::message_contents::{
        DmMembers as DmMembersProto, Inbox as InboxProto, OneshotMessage,
    };

    // Gated on the unified migration predicate — see
    // `merge_app_data_into_mutable_metadata` for the rationale.
    if !super::is_migrated_extensions(extensions) {
        return Ok(None);
    }

    let Some(ext) = extensions.app_data_dictionary() else {
        return Ok(None);
    };
    let dict = ext.dictionary();

    let Some(ct_bytes) = dict.get(&ComponentId::CONVERSATION_TYPE.as_u16()) else {
        return Ok(None);
    };
    let Some(creator_bytes) = dict.get(&ComponentId::CREATOR_INBOX_ID.as_u16()) else {
        return Ok(None);
    };

    let ct_arr: [u8; 4] =
        ct_bytes
            .try_into()
            .map_err(|_| ComponentSourceError::MalformedComponentValue {
                component_id: ComponentId::CONVERSATION_TYPE,
                reason: format!("expected 4 bytes, got {}", ct_bytes.len()),
            })?;
    let conversation_type = i32::from_be_bytes(ct_arr);

    let creator_inbox_id = InboxId::tls_deserialize_exact(creator_bytes)
        .map_err(|e| ComponentSourceError::MalformedComponentValue {
            component_id: ComponentId::CREATOR_INBOX_ID,
            reason: format!("invalid InboxId TLS encoding: {e}"),
        })?
        .to_hex();

    // `DM_MEMBERS` on the wire is `TlsSet<InboxId>`; re-shape to
    // `DmMembersProto` so downstream `GroupMetadata::try_from` is unchanged.
    let dm_members = match dict.get(&ComponentId::DM_MEMBERS.as_u16()) {
        Some(b) => {
            let set = TlsSet::<InboxId>::tls_deserialize_exact(b).map_err(|e| {
                ComponentSourceError::MalformedComponentValue {
                    component_id: ComponentId::DM_MEMBERS,
                    reason: format!("invalid TlsSet<InboxId>: {e}"),
                }
            })?;
            let ids: Vec<InboxId> = set.iter().copied().collect();
            if ids.len() != 2 {
                return Err(ComponentSourceError::MalformedComponentValue {
                    component_id: ComponentId::DM_MEMBERS,
                    reason: format!("expected 2 inbox ids, got {}", ids.len()),
                });
            }
            Some(DmMembersProto {
                dm_member_one: Some(InboxProto {
                    inbox_id: ids[0].to_hex(),
                }),
                dm_member_two: Some(InboxProto {
                    inbox_id: ids[1].to_hex(),
                }),
            })
        }
        None => None,
    };

    let oneshot = match dict.get(&ComponentId::ONESHOT_MESSAGE.as_u16()) {
        Some(b) => Some(OneshotMessage::decode(b).map_err(|e| {
            ComponentSourceError::MalformedComponentValue {
                component_id: ComponentId::ONESHOT_MESSAGE,
                reason: format!("OneshotMessage prost decode: {e}"),
            }
        })?),
        None => None,
    };

    Ok(Some(GroupMetadataReturn {
        conversation_type,
        creator_inbox_id,
        dm_members,
        oneshot,
    }))
}

/// Intermediate proto-shaped result of [`read_group_metadata_from_extensions`].
/// Caller converts to the final [`xmtp_mls_common::group_metadata::GroupMetadata`].
#[derive(Debug)]
pub(crate) struct GroupMetadataReturn {
    pub conversation_type: i32,
    pub creator_inbox_id: String,
    pub dm_members: Option<xmtp_proto::xmtp::mls::message_contents::DmMembers>,
    pub oneshot: Option<xmtp_proto::xmtp::mls::message_contents::OneshotMessage>,
}

/// Read the `GROUP_MEMBERSHIP` dict entry and decode it into the
/// legacy `GroupMembership` proto shape. Returns `Ok(None)` for
/// unmigrated groups. Used by `extract_group_membership` on the
/// receive-side validator to bridge the dict-stored membership back
/// into the existing `GroupMembership` Rust type without rewriting
/// every caller.
pub(crate) fn read_group_membership_from_dict(
    extensions: &Extensions<GroupContext>,
) -> Result<Option<xmtp_proto::xmtp::mls::message_contents::GroupMembership>, ComponentSourceError>
{
    use xmtp_mls_common::app_data::migration::decode_group_membership_dict;
    use xmtp_proto::xmtp::mls::message_contents::GroupMembership as GroupMembershipProto;

    // Gate on the unified migration predicate so a stray
    // `GROUP_MEMBERSHIP` dict entry on a pre-bootstrap group can't
    // shadow the authoritative legacy extension. Matches the gating
    // used by [`merge_app_data_into_mutable_metadata`] and the
    // `mutable_metadata()` / `is_super_admin_without_lock` callers.
    if !super::is_migrated_extensions(extensions) {
        return Ok(None);
    }

    let Some(ext) = extensions.app_data_dictionary() else {
        return Ok(None);
    };
    let Some(bytes) = ext
        .dictionary()
        .get(&ComponentId::GROUP_MEMBERSHIP.as_u16())
    else {
        return Ok(None);
    };

    let entries = decode_group_membership_dict(bytes).map_err(|e| {
        ComponentSourceError::MalformedComponentValue {
            component_id: ComponentId::GROUP_MEMBERSHIP,
            reason: format!("TlsMap decode: {e}"),
        }
    })?;

    // Flatten per-inbox GroupMembershipEntryV1 back into the legacy
    // proto shape: members (inbox_id → sequence_id), failed_installations
    // (flat Vec). The proto still has a flat failed_installations field
    // for backward compat — we concatenate per-inbox failed lists for
    // callers that still read the flat list.
    //
    // `decode_group_membership_dict` already rejects entries with
    // `version: None` (`MigrationError::GroupMembershipEntryUnknownVersion`),
    // so the only legal post-decode shape today is `Some(Version::V1(_))`.
    // Anything else (a future Version variant we can't interpret) is a
    // forward-compat hazard and surfaces as `MalformedComponentValue`.
    use xmtp_proto::xmtp::mls::message_contents::group_membership_entry::Version as GroupMembershipEntryVersion;
    let mut members: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
    let mut failed: Vec<Vec<u8>> = Vec::new();
    for (inbox_id, entry) in entries {
        let v1 = match entry.version {
            Some(GroupMembershipEntryVersion::V1(v1)) => v1,
            None => {
                return Err(ComponentSourceError::MalformedComponentValue {
                    component_id: ComponentId::GROUP_MEMBERSHIP,
                    reason: format!(
                        "GroupMembershipEntry for {} has no version",
                        inbox_id.to_hex()
                    ),
                });
            }
        };
        members.insert(inbox_id.to_hex(), v1.sequence_id);
        failed.extend(v1.failed_installations);
    }

    Ok(Some(GroupMembershipProto {
        members,
        failed_installations: failed,
    }))
}

/// Encode a list of hex inbox ids as a TLS-serialized `TlsSet<InboxId>`.
fn encode_inbox_id_set(inbox_ids: &[String]) -> Result<Vec<u8>, ComponentSourceError> {
    let ids: Vec<InboxId> = inbox_ids
        .iter()
        .map(|s| inbox_id_str_to_bytes(s))
        .collect::<Result<Vec<_>, _>>()?;
    let set: TlsSet<InboxId> = ids.into_iter().collect();
    Ok(set.tls_serialize_detached()?)
}

/// Wrap a single set mutation in a `TlsSetDelta` and serialize it.
fn encode_inbox_id_set_delta(
    mutation: TlsSetMutation<InboxId>,
) -> Result<Vec<u8>, ComponentSourceError> {
    let delta = TlsSetDelta::<InboxId> {
        mutations: vec![mutation],
    };
    Ok(delta.tls_serialize_detached()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tls_codec::VLBytes;
    use xmtp_mls_common::{
        app_data::{
            component_permissions::component_permissions,
            component_registry::{ComponentOp, new_component_metadata},
        },
        inbox_id::INBOX_ID_BYTE_LEN,
        tls_map::{TlsMap, TlsMapDelta},
        tls_set::TlsKeyHash,
    };
    use xmtp_proto::xmtp::mls::message_contents::{
        MetadataPolicy as MetadataPolicyProto,
        metadata_policy::{Kind as MetadataPolicyKind, MetadataBasePolicy},
    };

    /// Build a deterministic 64-character hex inbox id from a tag byte. The
    /// tag is repeated 32 times, giving a unique inbox id per call without
    /// needing real cryptographic generation.
    fn fake_inbox_id(tag: u8) -> String {
        hex::encode([tag; INBOX_ID_BYTE_LEN])
    }

    /// Build the [`InboxId`] form of [`fake_inbox_id`] directly.
    fn fake_inbox(tag: u8) -> InboxId {
        InboxId::from_bytes([tag; INBOX_ID_BYTE_LEN])
    }

    /// Empty registry constant for tests that exercise known-id paths —
    /// `lookup_component` resolves first, so the registry is never
    /// consulted and an empty one is sufficient.
    fn empty_registry() -> ComponentRegistry {
        ComponentRegistry::new()
    }

    /// Build a single-entry registry for tests that exercise the
    /// type-aware fallback on unknown ids. Permissions are `Allow` for
    /// every op so the policy layer does not interfere with the
    /// dispatch test under question.
    fn registry_with(id: ComponentId, ty: ComponentType) -> ComponentRegistry {
        fn allow() -> MetadataPolicyProto {
            MetadataPolicyProto {
                kind: Some(MetadataPolicyKind::Base(MetadataBasePolicy::Allow as i32)),
            }
        }
        let perms = component_permissions()
            .insert(allow())
            .update(allow())
            .delete(allow())
            .call();
        let meta = new_component_metadata(perms, ty);
        let mut reg = ComponentRegistry::new();
        reg.set(id, meta).unwrap();
        reg
    }

    // --- inbox-id helpers --------------------------------------------------

    #[xmtp_common::test]
    fn test_inbox_id_round_trip() {
        let original = fake_inbox_id(0xAB);
        let id = inbox_id_str_to_bytes(&original).unwrap();
        assert_eq!(id.as_bytes(), &[0xAB; 32]);
        assert_eq!(id.to_hex(), original);
    }

    #[xmtp_common::test]
    fn test_inbox_id_invalid_hex() {
        let err = inbox_id_str_to_bytes("not_hex").unwrap_err();
        assert!(
            matches!(
                err,
                ComponentSourceError::InvalidInboxId(InboxIdError::InvalidHex(_))
            ),
            "got {err:?}"
        );
    }

    #[xmtp_common::test]
    fn test_inbox_id_wrong_length() {
        // Valid hex but too short — only 16 bytes.
        let err = inbox_id_str_to_bytes(&"ab".repeat(16)).unwrap_err();
        assert!(
            matches!(
                err,
                ComponentSourceError::InvalidInboxId(InboxIdError::InvalidLength {
                    expected: INBOX_ID_BYTE_LEN,
                    actual: 16,
                })
            ),
            "got {err:?}"
        );
    }

    // --- component_type lookups --------------------------------------------

    #[xmtp_common::test]
    fn test_component_type_string_family() {
        // GMM-backed components whose wire format is UTF-8 text — names,
        // descriptions, URLs, and the app-data string blob.
        for id in [
            ComponentId::GROUP_NAME,
            ComponentId::GROUP_DESCRIPTION,
            ComponentId::GROUP_IMAGE_URL,
            ComponentId::APP_DATA,
            ComponentId::MIN_SUPPORTED_PROTOCOL_VERSION,
        ] {
            assert_eq!(component_type(id), Some(ComponentType::String));
        }
    }

    #[xmtp_common::test]
    fn test_component_type_bytes_family() {
        // Components whose payload is opaque bytes, not UTF-8 text:
        // timestamped disappearance windows and the commit-log signer key.
        for id in [
            ComponentId::MESSAGE_DISAPPEAR_FROM_NS,
            ComponentId::MESSAGE_DISAPPEAR_IN_NS,
            ComponentId::COMMIT_LOG_SIGNER,
        ] {
            assert_eq!(component_type(id), Some(ComponentType::Bytes));
        }
    }

    #[xmtp_common::test]
    fn test_component_type_set_inbox_id_family() {
        for id in [
            ComponentId::ADMIN_LIST,
            ComponentId::SUPER_ADMIN_LIST,
            ComponentId::DM_MEMBERS,
        ] {
            assert_eq!(component_type(id), Some(ComponentType::TlsSetInboxId));
        }
    }

    #[xmtp_common::test]
    fn test_component_type_group_membership() {
        assert_eq!(
            component_type(ComponentId::GROUP_MEMBERSHIP),
            Some(ComponentType::TlsMapInboxIdBytes)
        );
    }

    #[xmtp_common::test]
    fn test_component_type_app_range_is_none() {
        assert_eq!(component_type(ComponentId::new(0xC000)), None);
        assert_eq!(component_type(ComponentId::new(0xFDAB)), None);
    }

    // --- MetadataField <-> ComponentId mapping -----------------------------

    #[xmtp_common::test]
    fn test_metadata_field_round_trip() {
        for field in [
            MetadataField::GroupName,
            MetadataField::Description,
            MetadataField::GroupImageUrlSquare,
            MetadataField::MessageDisappearFromNS,
            MetadataField::MessageDisappearInNS,
            MetadataField::MinimumSupportedProtocolVersion,
            MetadataField::CommitLogSigner,
            MetadataField::AppData,
        ] {
            let id = metadata_field_to_component_id(field.as_str())
                .expect("every MetadataField has a ComponentId");
            let back = component_id_to_metadata_field(id)
                .expect("every mapped ComponentId has a MetadataField");
            assert_eq!(back, field, "round-trip mismatch for {field:?}");
        }
    }

    #[xmtp_common::test]
    fn test_metadata_field_unknown_name_returns_none() {
        assert!(metadata_field_to_component_id("nonexistent_field").is_none());
    }

    #[xmtp_common::test]
    fn test_component_id_to_metadata_field_non_gmm_returns_none() {
        assert!(component_id_to_metadata_field(ComponentId::ADMIN_LIST).is_none());
        assert!(component_id_to_metadata_field(ComponentId::GROUP_MEMBERSHIP).is_none());
        assert!(component_id_to_metadata_field(ComponentId::new(0xC000)).is_none());
    }

    // --- encode_app_data_update_payload ------------------------------------

    #[xmtp_common::test]
    fn test_encode_bytes_payload_passthrough() {
        let value = b"hello world";
        let payload = encode_app_data_update_payload(&ComponentMutation::Bytes {
            component_id: ComponentId::GROUP_NAME,
            new_value: value,
        })
        .unwrap();
        assert_eq!(payload, value);
    }

    #[xmtp_common::test]
    fn test_encode_bytes_payload_rejects_non_bytes_component() {
        // GROUP_MEMBERSHIP isn't a bytes component, so shoving a Bytes
        // mutation at it is a programming error — callers should build a
        // membership-specific mutation shape instead.
        let err = encode_app_data_update_payload(&ComponentMutation::Bytes {
            component_id: ComponentId::GROUP_MEMBERSHIP,
            new_value: b"x",
        })
        .unwrap_err();
        assert!(matches!(err, ComponentSourceError::MismatchedMutation(_)));
    }

    #[xmtp_common::test]
    fn test_encode_admin_list_insert_delta() {
        let inbox = fake_inbox_id(0x11);
        let payload =
            encode_app_data_update_payload(&ComponentMutation::AdminListAdd { inbox_id: &inbox })
                .unwrap();
        let delta = TlsSetDelta::<InboxId>::tls_deserialize_exact(&payload).unwrap();
        assert_eq!(delta.mutations.len(), 1);
        match &delta.mutations[0] {
            TlsSetMutation::Insert(k) => assert_eq!(*k, fake_inbox(0x11)),
            other => panic!("expected Insert, got {other:?}"),
        }
    }

    #[xmtp_common::test]
    fn test_encode_admin_list_remove_delta() {
        let inbox = fake_inbox_id(0x22);
        let payload = encode_app_data_update_payload(&ComponentMutation::AdminListRemove {
            inbox_id: &inbox,
        })
        .unwrap();
        let delta = TlsSetDelta::<InboxId>::tls_deserialize_exact(&payload).unwrap();
        assert_eq!(delta.mutations.len(), 1);
        match &delta.mutations[0] {
            TlsSetMutation::Remove(k) => assert_eq!(*k, fake_inbox(0x22)),
            other => panic!("expected Remove, got {other:?}"),
        }
    }

    #[xmtp_common::test]
    fn test_encode_super_admin_list_delta() {
        let inbox = fake_inbox_id(0x33);
        let add = encode_app_data_update_payload(&ComponentMutation::SuperAdminListAdd {
            inbox_id: &inbox,
        })
        .unwrap();
        let remove = encode_app_data_update_payload(&ComponentMutation::SuperAdminListRemove {
            inbox_id: &inbox,
        })
        .unwrap();
        let add_delta = TlsSetDelta::<InboxId>::tls_deserialize_exact(&add).unwrap();
        let remove_delta = TlsSetDelta::<InboxId>::tls_deserialize_exact(&remove).unwrap();
        assert!(matches!(&add_delta.mutations[0], TlsSetMutation::Insert(_)));
        assert!(matches!(
            &remove_delta.mutations[0],
            TlsSetMutation::Remove(_)
        ));
    }

    #[xmtp_common::test]
    fn test_encode_admin_list_invalid_inbox_id() {
        // "not-a-real-inbox-id" is non-hex, so the failure is the
        // hex-decode variant rather than the length variant.
        let err = encode_app_data_update_payload(&ComponentMutation::AdminListAdd {
            inbox_id: "not-a-real-inbox-id",
        })
        .unwrap_err();
        assert!(
            matches!(
                err,
                ComponentSourceError::InvalidInboxId(InboxIdError::InvalidHex(_))
            ),
            "got {err:?}"
        );
    }

    // --- apply_app_data_update_payload -------------------------------------

    #[xmtp_common::test]
    fn test_apply_bytes_payload_returns_payload_verbatim() {
        let payload = b"new_name";
        let new_value = apply_app_data_update_payload(
            ComponentId::GROUP_NAME,
            payload,
            None,
            &empty_registry(),
        )
        .unwrap();
        assert_eq!(new_value, payload);
    }

    #[xmtp_common::test]
    fn test_apply_bytes_payload_ignores_old_value() {
        // Full replacement — the old value is irrelevant.
        let new_value = apply_app_data_update_payload(
            ComponentId::GROUP_DESCRIPTION,
            b"replacement",
            Some(b"old_description"),
            &empty_registry(),
        )
        .unwrap();
        assert_eq!(new_value, b"replacement");
    }

    #[xmtp_common::test]
    fn test_apply_admin_list_insert_against_none() {
        // Apply an insert delta against a group that has no prior admin list.
        // The synthesized full value should be a TlsSet with the one inbox id.
        let inbox = fake_inbox_id(0x44);
        let insert_payload =
            encode_app_data_update_payload(&ComponentMutation::AdminListAdd { inbox_id: &inbox })
                .unwrap();

        let new_bytes = apply_app_data_update_payload(
            ComponentId::ADMIN_LIST,
            &insert_payload,
            None,
            &empty_registry(),
        )
        .unwrap();

        let set = TlsSet::<InboxId>::tls_deserialize_exact(&new_bytes).unwrap();
        assert_eq!(set.len(), 1);
        assert!(set.contains(&fake_inbox(0x44)));
    }

    #[xmtp_common::test]
    fn test_apply_admin_list_insert_against_existing_set() {
        // Build a prior set with one entry, then apply a delta that inserts a
        // second entry. Confirm both entries are present in the new value.
        let alice = fake_inbox_id(0x01);
        let bob = fake_inbox_id(0x02);

        let prior = encode_inbox_id_set(std::slice::from_ref(&alice)).unwrap();
        let insert_payload =
            encode_app_data_update_payload(&ComponentMutation::AdminListAdd { inbox_id: &bob })
                .unwrap();

        let new_bytes = apply_app_data_update_payload(
            ComponentId::ADMIN_LIST,
            &insert_payload,
            Some(&prior),
            &empty_registry(),
        )
        .unwrap();

        let set = TlsSet::<InboxId>::tls_deserialize_exact(&new_bytes).unwrap();
        assert_eq!(set.len(), 2);
        assert!(set.contains(&fake_inbox(0x01)));
        assert!(set.contains(&fake_inbox(0x02)));
    }

    #[xmtp_common::test]
    fn test_apply_admin_list_remove_against_existing_set() {
        let alice = fake_inbox_id(0x01);
        let bob = fake_inbox_id(0x02);

        let prior = encode_inbox_id_set(&[alice.clone(), bob.clone()]).unwrap();
        let remove_payload = encode_app_data_update_payload(&ComponentMutation::AdminListRemove {
            inbox_id: &alice,
        })
        .unwrap();

        let new_bytes = apply_app_data_update_payload(
            ComponentId::ADMIN_LIST,
            &remove_payload,
            Some(&prior),
            &empty_registry(),
        )
        .unwrap();

        let set = TlsSet::<InboxId>::tls_deserialize_exact(&new_bytes).unwrap();
        assert_eq!(set.len(), 1);
        assert!(set.contains(&fake_inbox(0x02)));
        assert!(!set.contains(&fake_inbox(0x01)));
    }

    #[xmtp_common::test]
    fn test_apply_super_admin_list_delta() {
        let owner = fake_inbox_id(0xAA);
        let new_sa = fake_inbox_id(0xBB);

        let prior = encode_inbox_id_set(std::slice::from_ref(&owner)).unwrap();
        let add_payload = encode_app_data_update_payload(&ComponentMutation::SuperAdminListAdd {
            inbox_id: &new_sa,
        })
        .unwrap();

        let new_bytes = apply_app_data_update_payload(
            ComponentId::SUPER_ADMIN_LIST,
            &add_payload,
            Some(&prior),
            &empty_registry(),
        )
        .unwrap();

        let set = TlsSet::<InboxId>::tls_deserialize_exact(&new_bytes).unwrap();
        assert_eq!(set.len(), 2);
        assert!(set.contains(&fake_inbox(0xAA)));
        assert!(set.contains(&fake_inbox(0xBB)));
    }

    #[xmtp_common::test]
    fn test_apply_admin_list_malformed_payload_returns_tls_codec_error() {
        // Garbage bytes that aren't a valid TlsSetDelta — exercises the
        // wire-format decode failure path that ProcessMessageWithAppDataError
        // ::AppDataDecode wraps in production. The receiver-side
        // `process_message_with_app_data` propagates this through the new
        // GroupMessageProcessingError::OpenMlsProcessMessageWithAppData
        // variant rather than masking it as `FoundAppDataUpdateProposal`.
        let err = apply_app_data_update_payload(
            ComponentId::ADMIN_LIST,
            &[0xff, 0xff, 0xff, 0xff],
            None,
            &empty_registry(),
        )
        .unwrap_err();
        assert!(
            matches!(err, ComponentSourceError::TlsCodec(_)),
            "got {err:?}"
        );
    }

    #[xmtp_common::test]
    fn test_apply_admin_list_malformed_old_value_returns_tls_codec_error() {
        // The delta payload is well-formed, but the old_value isn't a
        // valid TlsSet — also a TlsCodec error, just from the other side.
        let inbox = fake_inbox_id(0x55);
        let payload =
            encode_app_data_update_payload(&ComponentMutation::AdminListAdd { inbox_id: &inbox })
                .unwrap();
        let err = apply_app_data_update_payload(
            ComponentId::ADMIN_LIST,
            &payload,
            Some(&[0xde, 0xad, 0xbe, 0xef]),
            &empty_registry(),
        )
        .unwrap_err();
        assert!(
            matches!(err, ComponentSourceError::TlsCodec(_)),
            "got {err:?}"
        );
    }

    #[xmtp_common::test]
    fn test_apply_immutable_first_insert_allowed() {
        // Bootstrap-shape: an `Update(payload)` against an immutable
        // component with no prior value is the bootstrap commit's
        // first-write path. Apply must store the payload bytes
        // verbatim — the bootstrap validator's byte-compare catches a
        // peer that crafts a malicious initial value, so the apply
        // layer doesn't need its own decode/check step.
        let bytes = apply_app_data_update_payload(
            ComponentId::CONVERSATION_TYPE,
            b"seed",
            None,
            &empty_registry(),
        )
        .unwrap();
        assert_eq!(bytes, b"seed");
    }

    #[xmtp_common::test]
    fn test_apply_immutable_overwrite_rejected() {
        // Steady-state: an `Update(payload)` against an immutable
        // component that already has a prior value must fail. This is
        // the only path Byzantine peers have to mutate immutables
        // post-bootstrap, and the apply layer is the gatekeeper.
        let err = apply_app_data_update_payload(
            ComponentId::CONVERSATION_TYPE,
            b"junk",
            Some(b"prior"),
            &empty_registry(),
        )
        .unwrap_err();
        assert!(matches!(err, ComponentSourceError::ImmutableUpdate(_)));
    }

    #[xmtp_common::test]
    fn test_apply_component_registry_delta_against_empty() {
        // Bootstrap shape: a `TlsMapDelta<ComponentId, VLBytes>` of
        // all-`Insert` mutations applied against an empty map produces
        // a materialized snapshot containing those entries.
        let id_a = ComponentId::GROUP_NAME;
        let id_b = ComponentId::GROUP_DESCRIPTION;
        let delta = TlsMapDelta::<ComponentId, VLBytes>::new()
            .insert(id_a, VLBytes::new(vec![0x11; 4]))
            .insert(id_b, VLBytes::new(vec![0x22; 4]));
        let payload = delta.tls_serialize_detached().unwrap();

        let new_bytes = apply_app_data_update_payload(
            ComponentId::COMPONENT_REGISTRY,
            &payload,
            None,
            &empty_registry(),
        )
        .unwrap();
        let map = TlsMap::<ComponentId, VLBytes>::tls_deserialize_exact(&new_bytes).unwrap();
        assert_eq!(map.len(), 2);
        assert_eq!(
            map.get(&id_a).map(|v| v.as_slice()),
            Some([0x11; 4].as_slice())
        );
        assert_eq!(
            map.get(&id_b).map(|v| v.as_slice()),
            Some([0x22; 4].as_slice())
        );
    }

    #[xmtp_common::test]
    fn test_apply_group_membership_delta_against_existing_map() {
        // Post-bootstrap shape: an `Update` mutation applied on top of
        // a prior `TlsMap<InboxId, VLBytes>` snapshot produces a new
        // snapshot with the updated value.
        let alice = fake_inbox(0xAA);
        let bob = fake_inbox(0xBB);
        let mut prior: TlsMap<InboxId, VLBytes> = TlsMap::new();
        prior.set(alice, VLBytes::new(vec![0x01]));
        prior.set(bob, VLBytes::new(vec![0x02]));
        let prior_bytes = prior.tls_serialize_detached().unwrap();

        let delta = TlsMapDelta::<InboxId, VLBytes>::new()
            .update(alice, VLBytes::new(vec![0x99]))
            .delete(bob);
        let payload = delta.tls_serialize_detached().unwrap();

        let new_bytes = apply_app_data_update_payload(
            ComponentId::GROUP_MEMBERSHIP,
            &payload,
            Some(&prior_bytes),
            &empty_registry(),
        )
        .unwrap();
        let map = TlsMap::<InboxId, VLBytes>::tls_deserialize_exact(&new_bytes).unwrap();
        assert_eq!(map.len(), 1);
        assert_eq!(
            map.get(&alice).map(|v| v.as_slice()),
            Some([0x99].as_slice())
        );
        assert!(!map.contains_key(&bob));
    }

    #[xmtp_common::test]
    fn test_apply_map_component_malformed_delta_returns_codec_error() {
        // Garbage bytes that aren't a valid TlsMapDelta surface as a
        // TLS-codec error, same shape as the set-component path.
        let err = apply_app_data_update_payload(
            ComponentId::COMPONENT_REGISTRY,
            &[0xff, 0xff, 0xff, 0xff],
            None,
            &empty_registry(),
        )
        .unwrap_err();
        assert!(
            matches!(err, ComponentSourceError::TlsCodec(_)),
            "got {err:?}"
        );
    }

    #[xmtp_common::test]
    fn test_apply_map_component_apply_failure_surfaces_apply_error() {
        // A delta that updates a key not present in the prior snapshot
        // fails at apply time — surfaced as `TlsMapApply(KeyNotFound)`.
        let alice = fake_inbox(0x01);
        let delta = TlsMapDelta::<InboxId, VLBytes>::new().update(alice, VLBytes::new(vec![0x42]));
        let payload = delta.tls_serialize_detached().unwrap();
        let err = apply_app_data_update_payload(
            ComponentId::GROUP_MEMBERSHIP,
            &payload,
            None,
            &empty_registry(),
        )
        .unwrap_err();
        assert!(
            matches!(
                err,
                ComponentSourceError::TlsMapApply(TlsMapError::KeyNotFound)
            ),
            "got {err:?}"
        );
    }

    // --- Unknown-component tolerance via type-aware dispatch ---------------
    //
    // No per-id `Component` impl exists for these ids on this client; the
    // sender shipped a newer release. Old clients look up the
    // `ComponentType` registered for the id in the on-dict
    // [`ComponentRegistry`] and route the payload through the
    // type-level decoder. The six `ComponentType` variants cover the
    // wire-format universe, so any future well-known or
    // application-range component lands convergently — including
    // `TlsSet` / `TlsMap` deltas, which previously needed a per-id
    // impl to apply correctly.

    #[xmtp_common::test]
    fn test_apply_unknown_id_with_bytes_registry_entry_stores_payload() {
        // Bytes shape: opaque passthrough, registry entry supplies the
        // type tag so the dispatch knows *not* to try a TLS-delta
        // decode (which would corrupt the bytes).
        let id = ComponentId::new(0xC123);
        let registry = registry_with(id, ComponentType::Bytes);
        let new_value = apply_app_data_update_payload(id, b"opaque", None, &registry).unwrap();
        assert_eq!(new_value, b"opaque");
    }

    #[xmtp_common::test]
    fn test_apply_unknown_id_with_string_registry_entry_validates_utf8() {
        // String shape: payload must be valid UTF-8. Bad bytes surface
        // as `MalformedComponentValue` rather than silently corrupting
        // the dict.
        let id = ComponentId::new(0xC222);
        let registry = registry_with(id, ComponentType::String);
        let ok = apply_app_data_update_payload(id, b"hello", None, &registry).unwrap();
        assert_eq!(ok, b"hello");
        let err = apply_app_data_update_payload(id, &[0xC3, 0x28], None, &registry).unwrap_err();
        assert!(matches!(
            err,
            ComponentSourceError::MalformedComponentValue { .. }
        ));
    }

    #[xmtp_common::test]
    fn test_apply_unknown_id_with_tls_set_inbox_id_applies_delta() {
        // The whole point of registry-typed dispatch: a new
        // `TlsSet<InboxId>` component lands as a typed delta apply, not
        // as an opaque blob replacement — old and new clients converge
        // on the same `TlsSet` snapshot byte-for-byte.
        let id = ComponentId::new(0xC333);
        let registry = registry_with(id, ComponentType::TlsSetInboxId);
        let bob = fake_inbox(0x02);
        let delta = TlsSetDelta::<InboxId>::new().insert(bob);
        let payload = delta.tls_serialize_detached().unwrap();
        let new_bytes = apply_app_data_update_payload(id, &payload, None, &registry).unwrap();
        let set = TlsSet::<InboxId>::tls_deserialize_exact(&new_bytes).unwrap();
        assert_eq!(set.len(), 1);
        assert!(set.contains(&bob));
    }

    #[xmtp_common::test]
    fn test_apply_unknown_id_with_no_registry_entry_rejected() {
        // Deny-by-default: no per-id impl AND no registry entry means
        // we have no type to decode against. Reject rather than store
        // bytes whose shape we can't reason about — the alternative
        // would fork the dict the moment a typed client did know the
        // shape.
        let err =
            apply_app_data_update_payload(ComponentId::new(0xC456), b"x", None, &empty_registry())
                .unwrap_err();
        assert!(matches!(err, ComponentSourceError::UnknownComponent(_)));
    }

    /// Immutability is enforced range-by-id, not per-`Component`-impl,
    /// so an unknown id sitting in the XMTP immutable range
    /// (`0xBE00-0xBFFF`) still gets the bootstrap-style "first insert
    /// allowed, subsequent overwrite rejected" contract — even when
    /// the type-aware dispatcher (not a per-id `Component` impl) is
    /// the path being exercised. Pins the reviewer's concern that the
    /// tolerance branch might bypass immutability for newly-defined
    /// immutable components that a NEWER release ships.
    #[xmtp_common::test]
    fn test_apply_unknown_id_in_xmtp_immutable_range_first_write_allowed() {
        let id = ComponentId::new(0xBE05); // XMTP immutable range
        assert!(id.is_immutable());
        let registry = registry_with(id, ComponentType::Bytes);
        let new_value = apply_app_data_update_payload(id, b"seed", None, &registry).unwrap();
        assert_eq!(new_value, b"seed");
    }

    #[xmtp_common::test]
    fn test_apply_unknown_id_in_xmtp_immutable_range_overwrite_rejected() {
        let id = ComponentId::new(0xBE05);
        assert!(id.is_immutable());
        let registry = registry_with(id, ComponentType::Bytes);
        let err = apply_app_data_update_payload(id, b"new", Some(b"old"), &registry).unwrap_err();
        assert!(matches!(err, ComponentSourceError::ImmutableUpdate(_)));
    }

    /// Same contract for the application immutable range
    /// (`0xFD00-0xFEFF`) — overwrites of an unknown immutable
    /// application component are rejected even though no per-id impl
    /// exists on this client.
    #[xmtp_common::test]
    fn test_apply_unknown_id_in_app_immutable_range_overwrite_rejected() {
        let id = ComponentId::new(0xFD42); // app immutable range
        assert!(id.is_immutable());
        let registry = registry_with(id, ComponentType::Bytes);
        let err = apply_app_data_update_payload(id, b"new", Some(b"old"), &registry).unwrap_err();
        assert!(matches!(err, ComponentSourceError::ImmutableUpdate(_)));
    }

    #[xmtp_common::test]
    fn test_apply_reserved_range_component_rejected_with_or_without_registry() {
        // 0xFF00+ is the reserved range. `ComponentRegistry::set`
        // refuses to insert reserved ids, so a registry entry can't
        // even be constructed — the apply path falls through to
        // `UnknownComponent`.
        let err =
            apply_app_data_update_payload(ComponentId::new(0xFF01), b"x", None, &empty_registry())
                .unwrap_err();
        assert!(matches!(err, ComponentSourceError::UnknownComponent(_)));
    }

    #[xmtp_common::test]
    fn test_apply_out_of_range_component_rejected() {
        // Ids below 0x8000 are outside the AppData address space.
        let err =
            apply_app_data_update_payload(ComponentId::new(0x0042), b"x", None, &empty_registry())
                .unwrap_err();
        assert!(matches!(err, ComponentSourceError::UnknownComponent(_)));
    }

    #[xmtp_common::test]
    fn test_apply_unknown_xmtp_range_id_with_registry_dispatches() {
        // Same path applies to the XMTP-defined range, not just the
        // app range: any 0x8000-0xBFFF id that lacks a per-id impl
        // routes through the registry.
        let id = ComponentId::new(0x8FFF);
        assert!(id.is_xmtp_range());
        let registry = registry_with(id, ComponentType::Bytes);
        let new_value = apply_app_data_update_payload(id, b"opaque", None, &registry).unwrap();
        assert_eq!(new_value, b"opaque");
    }

    // --- expand_app_data_update_to_changes ---------------------------------

    /// Helper: wrap a `TlsSetDelta<InboxId>` payload in an
    /// `AppDataUpdateOperation::Update(...)`.
    fn update_op(delta: TlsSetDelta<InboxId>) -> AppDataUpdateOperation {
        AppDataUpdateOperation::Update(delta.tls_serialize_detached().unwrap().into())
    }

    #[xmtp_common::test]
    fn test_expand_insert_surfaces_inbox_id_bytes() {
        let alice = fake_inbox(0x11);
        let delta: TlsSetDelta<InboxId> = TlsSetDelta {
            mutations: vec![TlsSetMutation::Insert(alice)],
        };
        let changes = expand_app_data_update_to_changes(
            ComponentId::ADMIN_LIST,
            &update_op(delta),
            None,
            &empty_registry(),
        )
        .unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].op, ComponentOp::Insert);
        assert_eq!(
            changes[0].value.as_deref(),
            Some(alice.as_bytes().as_slice())
        );
    }

    #[xmtp_common::test]
    fn test_expand_remove_surfaces_inbox_id_bytes() {
        let alice = fake_inbox(0x22);
        let delta: TlsSetDelta<InboxId> = TlsSetDelta {
            mutations: vec![TlsSetMutation::Remove(alice)],
        };
        let changes = expand_app_data_update_to_changes(
            ComponentId::ADMIN_LIST,
            &update_op(delta),
            None,
            &empty_registry(),
        )
        .unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].op, ComponentOp::Delete);
        assert_eq!(
            changes[0].value.as_deref(),
            Some(alice.as_bytes().as_slice())
        );
    }

    #[xmtp_common::test]
    fn test_expand_remove_by_hash_resolves_to_inbox_id_from_old_value() {
        // Prior set has alice + bob; RemoveByHash(hash(alice)) should
        // resolve back to alice's raw 32 bytes so the validator sees
        // *which* identity is being removed.
        let alice = fake_inbox(0xAA);
        let bob = fake_inbox(0xBB);
        let prior: TlsSet<InboxId> = [alice, bob].into_iter().collect();
        let old_bytes = prior.tls_serialize_detached().unwrap();

        let hash = TlsKeyHash::of(&alice).unwrap();
        let delta: TlsSetDelta<InboxId> = TlsSetDelta {
            mutations: vec![TlsSetMutation::RemoveByHash(hash)],
        };
        let changes = expand_app_data_update_to_changes(
            ComponentId::ADMIN_LIST,
            &update_op(delta),
            Some(&old_bytes),
            &empty_registry(),
        )
        .unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].op, ComponentOp::Delete);
        assert_eq!(
            changes[0].value.as_deref(),
            Some(alice.as_bytes().as_slice())
        );
    }

    #[xmtp_common::test]
    fn test_expand_remove_by_hash_miss_surfaces_none_value() {
        // Prior set has alice; RemoveByHash targets bob's hash (not in set).
        // Expansion surfaces value: None — the CRDT apply step will later
        // reject with KeyNotFound. Expansion's job is reshape, not auth.
        let alice = fake_inbox(0x01);
        let bob = fake_inbox(0x02);
        let prior: TlsSet<InboxId> = [alice].into_iter().collect();
        let old_bytes = prior.tls_serialize_detached().unwrap();

        let hash = TlsKeyHash::of(&bob).unwrap();
        let delta: TlsSetDelta<InboxId> = TlsSetDelta {
            mutations: vec![TlsSetMutation::RemoveByHash(hash)],
        };
        let changes = expand_app_data_update_to_changes(
            ComponentId::ADMIN_LIST,
            &update_op(delta),
            Some(&old_bytes),
            &empty_registry(),
        )
        .unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].op, ComponentOp::Delete);
        assert!(changes[0].value.is_none());
    }

    #[xmtp_common::test]
    fn test_expand_remove_by_hash_with_no_old_value_surfaces_none() {
        // No prior bytes → no set to search → every RemoveByHash misses.
        let hash = TlsKeyHash::of(&fake_inbox(0x33)).unwrap();
        let delta: TlsSetDelta<InboxId> = TlsSetDelta {
            mutations: vec![TlsSetMutation::RemoveByHash(hash)],
        };
        let changes = expand_app_data_update_to_changes(
            ComponentId::SUPER_ADMIN_LIST,
            &update_op(delta),
            None,
            &empty_registry(),
        )
        .unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].op, ComponentOp::Delete);
        assert!(changes[0].value.is_none());
    }

    #[xmtp_common::test]
    fn test_expand_remove_by_hash_malformed_old_value_surfaces_codec_error() {
        // Our own dict bytes are corrupt — surface the decode error
        // loudly rather than silently degrading to value: None, since a
        // corrupt prior set signals a local-state bug, not a peer bug.
        let hash = TlsKeyHash::of(&fake_inbox(0x44)).unwrap();
        let delta: TlsSetDelta<InboxId> = TlsSetDelta {
            mutations: vec![TlsSetMutation::RemoveByHash(hash)],
        };
        let err = expand_app_data_update_to_changes(
            ComponentId::ADMIN_LIST,
            &update_op(delta),
            Some(&[0xde, 0xad, 0xbe, 0xef]),
            &empty_registry(),
        )
        .unwrap_err();
        assert!(
            matches!(err, ComponentSourceError::TlsCodec(_)),
            "got {err:?}"
        );
    }

    /// XIP §2.2: an unknown component id with `Update(payload)` and a
    /// registered [`ComponentType::Bytes`] expands to a single Update
    /// change carrying the payload, so registry-policy validation runs
    /// against bytes a typed client would also accept opaquely.
    #[xmtp_common::test]
    fn test_expand_unknown_component_bytes_typed_emits_single_update() {
        let id = ComponentId::new(0x80FF);
        let registry = registry_with(id, ComponentType::Bytes);
        let changes = expand_app_data_update_to_changes(
            id,
            &AppDataUpdateOperation::Update(b"opaque".to_vec().into()),
            None,
            &registry,
        )
        .unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].op, ComponentOp::Update);
        assert_eq!(changes[0].value.as_deref(), Some(b"opaque".as_slice()));
    }

    /// Unknown id with `Remove` and any registered type expands to a
    /// single Delete change — the wipe semantics are type-agnostic.
    #[xmtp_common::test]
    fn test_expand_unknown_component_remove_emits_single_delete() {
        let id = ComponentId::new(0x80FF);
        let registry = registry_with(id, ComponentType::Bytes);
        let changes =
            expand_app_data_update_to_changes(id, &AppDataUpdateOperation::Remove, None, &registry)
                .unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].op, ComponentOp::Delete);
        assert!(changes[0].value.is_none());
    }

    /// Type-aware dispatch is the whole point: an unknown
    /// `TlsSet<InboxId>` component expands to a per-element change
    /// list, not a single opaque blob — so the validator policy loop
    /// inspects each `Insert` individually, the same as it would for a
    /// known set-shaped component.
    #[xmtp_common::test]
    fn test_expand_unknown_component_tls_set_inbox_id_emits_per_element_changes() {
        let id = ComponentId::new(0x80FE);
        let registry = registry_with(id, ComponentType::TlsSetInboxId);
        let alice = fake_inbox(0x11);
        let bob = fake_inbox(0x22);
        let delta = TlsSetDelta::<InboxId>::new().insert(alice).insert(bob);
        let payload = delta.tls_serialize_detached().unwrap();
        let changes = expand_app_data_update_to_changes(
            id,
            &AppDataUpdateOperation::Update(payload.into()),
            None,
            &registry,
        )
        .unwrap();
        assert_eq!(changes.len(), 2);
        assert!(changes.iter().all(|c| c.op == ComponentOp::Insert));
    }

    /// Reserved-range ids are not tolerated. `ComponentRegistry::set`
    /// rejects reserved ids at construction time, so a registry entry
    /// can't even be built — the dispatcher falls through to the
    /// `UnknownComponent` rejection.
    #[xmtp_common::test]
    fn test_expand_reserved_range_id_still_rejected() {
        let err = expand_app_data_update_to_changes(
            ComponentId::new(0xFF02),
            &AppDataUpdateOperation::Update(b"x".to_vec().into()),
            None,
            &empty_registry(),
        )
        .unwrap_err();
        assert!(matches!(err, ComponentSourceError::UnknownComponent(_)));
    }

    /// Deny-by-default: an unknown id with no registry entry is
    /// rejected. Without a type tag we have no idea how to decode the
    /// payload, and storing it opaquely would diverge from any future
    /// typed client that DID know the shape.
    #[xmtp_common::test]
    fn test_expand_unknown_component_no_registry_entry_rejected() {
        let err = expand_app_data_update_to_changes(
            ComponentId::new(0x80FF),
            &AppDataUpdateOperation::Update(b"opaque".to_vec().into()),
            None,
            &empty_registry(),
        )
        .unwrap_err();
        assert!(matches!(err, ComponentSourceError::UnknownComponent(_)));
    }

    #[xmtp_common::test]
    fn test_expand_skips_old_value_decode_when_no_remove_by_hash() {
        // Delta has only Insert/Remove — we should never touch old_value,
        // so even a garbage old_value must not fail the expansion.
        let alice = fake_inbox(0x55);
        let delta: TlsSetDelta<InboxId> = TlsSetDelta {
            mutations: vec![TlsSetMutation::Insert(alice)],
        };
        let changes = expand_app_data_update_to_changes(
            ComponentId::ADMIN_LIST,
            &update_op(delta),
            Some(&[0xff, 0xff, 0xff]), // intentionally malformed
            &empty_registry(),
        )
        .unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].op, ComponentOp::Insert);
    }

    // ========================================================================
    // Dict-reader helpers — happy path / unmigrated / malformed coverage
    // ========================================================================
    //
    // The three `read_*_from_dict` helpers execute before bootstrap is
    // wired end-to-end; the unit tests below pin the wire-format
    // contract so a decoder drift can't silently ship a broken read path.

    use openmls::extensions::{
        AppDataDictionary, AppDataDictionaryExtension, Extension as OpenMlsExtension, Extensions,
    };

    /// Build a synthetic `Extensions<GroupContext>` that carries only
    /// an `AppDataDictionary`. `migrated=true` seeds
    /// `COMPONENT_REGISTRY` with placeholder bytes so
    /// `is_migrated_extensions` returns true; `migrated=false` leaves
    /// the marker absent.
    fn extensions_with_entries(
        migrated: bool,
        entries: &[(u16, Vec<u8>)],
    ) -> Extensions<openmls::group::GroupContext> {
        let mut dict = AppDataDictionary::new();
        if migrated {
            let _ = dict.insert(ComponentId::COMPONENT_REGISTRY.as_u16(), vec![0xCA; 4]);
        }
        for (id, bytes) in entries {
            let _ = dict.insert(*id, bytes.clone());
        }
        Extensions::from_vec(vec![OpenMlsExtension::AppDataDictionary(
            AppDataDictionaryExtension::new(dict),
        )])
        .expect("valid group-context extension set")
    }

    // --- read_super_admin_list_from_extensions ------------------------------

    #[xmtp_common::test]
    fn read_super_admin_list_unmigrated_returns_none() {
        // No COMPONENT_REGISTRY marker => unmigrated, overlay stays off
        // even if SUPER_ADMIN_LIST bytes happen to exist.
        let exts = extensions_with_entries(
            false,
            &[(
                ComponentId::SUPER_ADMIN_LIST.as_u16(),
                encode_inbox_id_set(&[fake_inbox_id(0x11)]).unwrap(),
            )],
        );
        assert!(
            read_super_admin_list_from_extensions(&exts)
                .unwrap()
                .is_none()
        );
    }

    #[xmtp_common::test]
    fn read_super_admin_list_migrated_absent_returns_none() {
        // Migrated group but the dict has no SUPER_ADMIN_LIST entry —
        // `Ok(None)` rather than surfacing a malformed-value error.
        let exts = extensions_with_entries(true, &[]);
        assert!(
            read_super_admin_list_from_extensions(&exts)
                .unwrap()
                .is_none()
        );
    }

    #[xmtp_common::test]
    fn read_super_admin_list_migrated_happy_path() {
        let ids = vec![fake_inbox_id(0xAA), fake_inbox_id(0xBB)];
        let bytes = encode_inbox_id_set(&ids).unwrap();
        let exts =
            extensions_with_entries(true, &[(ComponentId::SUPER_ADMIN_LIST.as_u16(), bytes)]);
        let got = read_super_admin_list_from_extensions(&exts)
            .unwrap()
            .unwrap();
        assert_eq!(got.len(), 2);
        // TlsSet sorts by value so 0xAA sorts before 0xBB.
        assert_eq!(got[0], fake_inbox_id(0xAA));
        assert_eq!(got[1], fake_inbox_id(0xBB));
    }

    #[xmtp_common::test]
    fn read_super_admin_list_malformed_bytes_surface_error() {
        let exts = extensions_with_entries(
            true,
            &[(
                ComponentId::SUPER_ADMIN_LIST.as_u16(),
                vec![0x00, 0xDE, 0xAD],
            )],
        );
        let err = read_super_admin_list_from_extensions(&exts).unwrap_err();
        assert!(matches!(
            err,
            ComponentSourceError::MalformedComponentValue {
                component_id,
                ..
            } if component_id == ComponentId::SUPER_ADMIN_LIST
        ));
    }

    // --- read_group_metadata_from_extensions --------------------------------

    fn encode_conv_type_bytes(value: i32) -> Vec<u8> {
        value.to_be_bytes().to_vec()
    }

    fn encode_dm_pair(tag_a: u8, tag_b: u8) -> Vec<u8> {
        encode_inbox_id_set(&[fake_inbox_id(tag_a), fake_inbox_id(tag_b)]).unwrap()
    }

    /// Encode a single tagged inbox id in the `CREATOR_INBOX_ID` wire
    /// form (versioned `InboxId` TLS encoding).
    fn encode_creator_bytes(tag: u8) -> Vec<u8> {
        fake_inbox(tag).tls_serialize_detached().unwrap()
    }

    #[xmtp_common::test]
    fn read_group_metadata_unmigrated_returns_none() {
        let exts = extensions_with_entries(
            false,
            &[
                (
                    ComponentId::CONVERSATION_TYPE.as_u16(),
                    encode_conv_type_bytes(1),
                ),
                (
                    ComponentId::CREATOR_INBOX_ID.as_u16(),
                    encode_creator_bytes(0x11),
                ),
            ],
        );
        assert!(
            read_group_metadata_from_extensions(&exts)
                .unwrap()
                .is_none()
        );
    }

    #[xmtp_common::test]
    fn read_group_metadata_missing_required_seeds_returns_none() {
        // Migrated group but CONVERSATION_TYPE is absent — treat as
        // "seeds not ready yet" (Ok(None)) rather than malformed.
        let exts = extensions_with_entries(true, &[]);
        assert!(
            read_group_metadata_from_extensions(&exts)
                .unwrap()
                .is_none()
        );
    }

    #[xmtp_common::test]
    fn read_group_metadata_happy_path_non_dm() {
        let exts = extensions_with_entries(
            true,
            &[
                (
                    ComponentId::CONVERSATION_TYPE.as_u16(),
                    encode_conv_type_bytes(1),
                ),
                (
                    ComponentId::CREATOR_INBOX_ID.as_u16(),
                    encode_creator_bytes(0x11),
                ),
            ],
        );
        let got = read_group_metadata_from_extensions(&exts).unwrap().unwrap();
        assert_eq!(got.conversation_type, 1);
        assert_eq!(got.creator_inbox_id, fake_inbox_id(0x11));
        assert!(got.dm_members.is_none());
        assert!(got.oneshot.is_none());
    }

    #[xmtp_common::test]
    fn read_group_metadata_dm_happy_path() {
        // DM group — DM_MEMBERS decodes as TlsSet<InboxId>, re-shaped
        // to the proto's two-slot form.
        let exts = extensions_with_entries(
            true,
            &[
                (
                    ComponentId::CONVERSATION_TYPE.as_u16(),
                    encode_conv_type_bytes(2),
                ),
                (
                    ComponentId::CREATOR_INBOX_ID.as_u16(),
                    encode_creator_bytes(0x22),
                ),
                (ComponentId::DM_MEMBERS.as_u16(), encode_dm_pair(0x22, 0x33)),
            ],
        );
        let got = read_group_metadata_from_extensions(&exts).unwrap().unwrap();
        let dm = got.dm_members.unwrap();
        assert_eq!(dm.dm_member_one.unwrap().inbox_id, fake_inbox_id(0x22));
        assert_eq!(dm.dm_member_two.unwrap().inbox_id, fake_inbox_id(0x33));
    }

    #[xmtp_common::test]
    fn read_group_metadata_dm_wrong_cardinality_errors() {
        // A 1-element TlsSet<InboxId> is invalid for DM_MEMBERS —
        // surfaces `MalformedComponentValue`.
        let one_element = encode_inbox_id_set(&[fake_inbox_id(0x44)]).unwrap();
        let exts = extensions_with_entries(
            true,
            &[
                (
                    ComponentId::CONVERSATION_TYPE.as_u16(),
                    encode_conv_type_bytes(2),
                ),
                (
                    ComponentId::CREATOR_INBOX_ID.as_u16(),
                    encode_creator_bytes(0x44),
                ),
                (ComponentId::DM_MEMBERS.as_u16(), one_element),
            ],
        );
        let err = read_group_metadata_from_extensions(&exts).unwrap_err();
        assert!(matches!(
            err,
            ComponentSourceError::MalformedComponentValue {
                component_id,
                ..
            } if component_id == ComponentId::DM_MEMBERS
        ));
    }

    #[xmtp_common::test]
    fn read_group_metadata_malformed_creator_errors() {
        // CREATOR_INBOX_ID is the versioned `InboxId` TLS encoding —
        // a few stray bytes won't satisfy the varint length prefix
        // plus 32-byte payload, so deserialization must fail loud as
        // `MalformedComponentValue` rather than silently producing
        // a phantom inbox id.
        let exts = extensions_with_entries(
            true,
            &[
                (
                    ComponentId::CONVERSATION_TYPE.as_u16(),
                    encode_conv_type_bytes(1),
                ),
                (
                    ComponentId::CREATOR_INBOX_ID.as_u16(),
                    vec![0xFF, 0xFE, 0xFD],
                ),
            ],
        );
        let err = read_group_metadata_from_extensions(&exts).unwrap_err();
        assert!(matches!(
            err,
            ComponentSourceError::MalformedComponentValue {
                component_id,
                ..
            } if component_id == ComponentId::CREATOR_INBOX_ID
        ));
    }

    // --- read_group_membership_from_dict ------------------------------------

    #[xmtp_common::test]
    fn read_group_membership_unmigrated_returns_none() {
        use std::collections::BTreeMap;
        use xmtp_mls_common::app_data::migration::encode_group_membership_dict;
        use xmtp_proto::xmtp::mls::message_contents::{
            GroupMembershipEntry,
            group_membership_entry::{V1 as GroupMembershipEntryV1, Version},
        };
        let mut entries: BTreeMap<InboxId, GroupMembershipEntry> = BTreeMap::new();
        entries.insert(
            InboxId::from_bytes([0x11; INBOX_ID_BYTE_LEN]),
            GroupMembershipEntry {
                version: Some(Version::V1(GroupMembershipEntryV1 {
                    sequence_id: 1,
                    failed_installations: vec![],
                })),
            },
        );
        let bytes = encode_group_membership_dict(&entries).unwrap();
        let exts =
            extensions_with_entries(false, &[(ComponentId::GROUP_MEMBERSHIP.as_u16(), bytes)]);
        assert!(read_group_membership_from_dict(&exts).unwrap().is_none());
    }

    #[xmtp_common::test]
    fn read_group_membership_happy_path_flattens_per_inbox() {
        use std::collections::BTreeMap;
        use xmtp_mls_common::app_data::migration::encode_group_membership_dict;
        use xmtp_proto::xmtp::mls::message_contents::{
            GroupMembershipEntry,
            group_membership_entry::{V1 as GroupMembershipEntryV1, Version},
        };
        let mut entries: BTreeMap<InboxId, GroupMembershipEntry> = BTreeMap::new();
        entries.insert(
            InboxId::from_bytes([0x11; INBOX_ID_BYTE_LEN]),
            GroupMembershipEntry {
                version: Some(Version::V1(GroupMembershipEntryV1 {
                    sequence_id: 7,
                    failed_installations: vec![vec![0xA1; 16]],
                })),
            },
        );
        entries.insert(
            InboxId::from_bytes([0x22; INBOX_ID_BYTE_LEN]),
            GroupMembershipEntry {
                version: Some(Version::V1(GroupMembershipEntryV1 {
                    sequence_id: 42,
                    failed_installations: vec![vec![0xB1; 16]],
                })),
            },
        );
        let bytes = encode_group_membership_dict(&entries).unwrap();
        let exts =
            extensions_with_entries(true, &[(ComponentId::GROUP_MEMBERSHIP.as_u16(), bytes)]);

        let proto = read_group_membership_from_dict(&exts).unwrap().unwrap();
        // `members` is flat <hex_inbox_id, seq>
        assert_eq!(proto.members.len(), 2);
        assert_eq!(proto.members.get(&fake_inbox_id(0x11)), Some(&7));
        assert_eq!(proto.members.get(&fake_inbox_id(0x22)), Some(&42));
        // `failed_installations` concatenates the per-inbox lists.
        assert_eq!(proto.failed_installations.len(), 2);
    }

    #[xmtp_common::test]
    fn read_group_membership_malformed_bytes_surface_error() {
        let exts = extensions_with_entries(
            true,
            &[(
                ComponentId::GROUP_MEMBERSHIP.as_u16(),
                vec![0xDE, 0xAD, 0xBE, 0xEF],
            )],
        );
        let err = read_group_membership_from_dict(&exts).unwrap_err();
        assert!(matches!(
            err,
            ComponentSourceError::MalformedComponentValue {
                component_id,
                ..
            } if component_id == ComponentId::GROUP_MEMBERSHIP
        ));
    }

    // ========================================================================
    // merge_app_data_into_mutable_metadata_from_extensions —
    //   independence from COMPONENT_REGISTRY parseability
    // ========================================================================
    //
    // These pin the call-graph invariant that a malformed
    // `COMPONENT_REGISTRY` does **not** cause `mutable_metadata()` to
    // drop authoritative dict-backed fields. The migration-marker check
    // (`is_migrated_extensions`) uses key existence; the merge function
    // reads each metadata field directly from the dict; nothing on this
    // read path calls `load_component_registry`. Registry corruption is
    // surfaced loudly on the *write* paths (sender gate in `mls_sync.rs`
    // and the commit validator in `validated_commit.rs`), where it
    // belongs.
    //
    // Note: the existing `extensions_with_entries(migrated=true, …)`
    // helper already seeds `COMPONENT_REGISTRY` with placeholder bytes
    // (`vec![0xCA; 4]`) that don't decode as a valid registry, so every
    // migrated-test in this file already exercises the malformed-
    // registry branch implicitly. The tests below pin it explicitly so
    // a reviewer doesn't have to chase the helper to verify.

    use xmtp_mls_common::group_mutable_metadata::{GroupMutableMetadata, MetadataField};

    fn empty_base_gmm() -> GroupMutableMetadata {
        GroupMutableMetadata::new(std::collections::HashMap::new(), Vec::new(), Vec::new())
    }

    #[xmtp_common::test]
    fn merge_with_malformed_registry_returns_valid_field() {
        // Reviewer's alleged "data loss" scenario: COMPONENT_REGISTRY
        // contains malformed bytes, but a metadata field (GROUP_NAME)
        // is present and valid. The merge function does NOT validate
        // the registry — it reads GROUP_NAME directly — so the result
        // must contain "My Group" with no error.
        let exts = extensions_with_entries(
            true, // seeds COMPONENT_REGISTRY with non-decodable 0xCA bytes
            &[(ComponentId::GROUP_NAME.as_u16(), b"My Group".to_vec())],
        );
        let mut base = empty_base_gmm();
        merge_app_data_into_mutable_metadata_from_extensions(&mut base, &exts)
            .expect("merge ignores registry parseability and reads field directly");
        assert_eq!(
            base.attributes
                .get(MetadataField::GroupName.as_str())
                .map(String::as_str),
            Some("My Group"),
        );
    }

    #[xmtp_common::test]
    fn merge_unmigrated_is_noop() {
        // Sanity: pre-migration, the merge gate stays closed — even
        // if a stray dict entry exists, the base GMM is left untouched
        // so legacy GMM remains authoritative.
        let exts = extensions_with_entries(
            false,
            &[(ComponentId::GROUP_NAME.as_u16(), b"Stray".to_vec())],
        );
        let mut base = empty_base_gmm();
        merge_app_data_into_mutable_metadata_from_extensions(&mut base, &exts).unwrap();
        assert!(
            !base
                .attributes
                .contains_key(MetadataField::GroupName.as_str()),
            "merge must be a no-op on unmigrated extensions"
        );
    }

    #[xmtp_common::test]
    fn merge_with_malformed_field_surfaces_error_not_silent_loss() {
        // The other half of the invariant: when a metadata field's bytes
        // ARE malformed, the merge fails loudly with
        // `MalformedComponentValue` carrying the offending component id —
        // it never silently swallows the value. Pairs with the test
        // above to disprove "all metadata may be lost" framings.
        let exts = extensions_with_entries(
            true,
            &[(ComponentId::ADMIN_LIST.as_u16(), vec![0xff, 0xff, 0xff])],
        );
        let mut base = empty_base_gmm();
        let err =
            merge_app_data_into_mutable_metadata_from_extensions(&mut base, &exts).unwrap_err();
        assert!(
            matches!(
                err,
                ComponentSourceError::MalformedComponentValue { component_id, .. }
                    if component_id == ComponentId::ADMIN_LIST
            ),
            "expected MalformedComponentValue for ADMIN_LIST, got: {err:?}"
        );
    }
}
