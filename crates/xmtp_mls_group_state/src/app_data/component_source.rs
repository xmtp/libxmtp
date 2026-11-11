//! Single source-of-truth for the per-`ComponentId` read/encode logic and
//! the byte-level apply.
//!
//! Centralizes everything the rest of the MLS pipeline needs to know about
//! a well-known `ComponentId`: its logical [`ComponentType`], where its
//! current bytes live (OpenMLS AppData dictionary vs. legacy group context
//! extensions), and how to encode/apply `AppDataUpdate` payloads.
//!
//! The helpers are `pub` so `xmtp_mls` can call them. They read and write
//! raw component bytes; `xmtp_mls` owns the intent and commit paths that
//! decide when a write is allowed.
//!
//! ## Two apply paths
//!
//! [`apply_app_data_update_payload`] decodes, applies, and re-encodes the
//! whole component for each update. The
//! [`WorkingSet`](super::working_set::WorkingSet) applies the same updates
//! to components it decodes once per commit, and applies a collection by
//! its [`ComponentType`] rather than through its `Component` impl. Members
//! must reach one verdict on every input, so a change to how a component
//! applies changes both paths. The differential test in
//! `working_set/tests/differential.rs` compares them.
//!
//! ## Field bounds
//!
//! [`apply_app_data_update_payload`] rejects a new value of an application
//! component, `USER_DISPLAY_NAME`, or `GROUP_IMAGE` whose scalar,
//! collection key, or collection value exceeds [`MAX_FIELD_ELEMENT_BYTES`],
//! or whose serialized collection exceeds [`MAX_FIELD_SNAPSHOT_BYTES`].
//! [`check_update_payload_bounds`] applies the element bound to every
//! element an update payload names, without the state it applies to.
//!
//! ## Inbox-id encoding
//!
//! The legacy GMM extension stores inbox ids as 64-character hex strings.
//! Anything serialized through the new `AppDataUpdate` path uses the
//! versioned [`InboxId`] newtype.
//!
//! See [`crate::inbox_id`] for the full wire-format contract;
//! the short version is `varint(version) || 32-byte payload`, with
//! version 0 producing a 33-byte encoding.
//!
//! ## Secret values
//!
//! `GROUP_ROOT_KEY` holds the group root, and `COMMIT_LOG_SIGNER` the
//! commit-log private key of a legacy group, so every component value these
//! helpers copy is a secret candidate. Reads borrow
//! the dictionary's bytes, and computed values come back in `Zeroizing`,
//! which wipes them on drop whatever the component.

use crate::{
    app_data::{
        component_id::ComponentId,
        component_registry::ComponentRegistry,
        components::type_dispatch::{apply_update_payload_for_type, expand_to_changes_for_type},
        registry_table::lookup_component,
        typed::ComponentTypedError,
        validation::ComponentWriteValidator,
    },
    group_mutable_metadata::{GroupMutableMetadata, GroupMutableMetadataError, MetadataField},
    inbox_id::{InboxId, InboxIdError},
};
use openmls::{
    extensions::Extensions,
    group::{GroupContext, MlsGroup as OpenMlsGroup},
    messages::proposals::AppDataUpdateOperation,
};
use tls_codec::{Deserialize, Serialize, VLBytes};
use xmtp_cryptography::secret::Zeroizing;
use xmtp_mls_group_keys::group_keys::GROUP_ROOT_BYTES;
use xmtp_mls_tls_collections::{
    tls_map::{TlsMap, TlsMapDelta, TlsMapError, TlsMapMutation},
    tls_set::{TlsSet, TlsSetDelta, TlsSetError, TlsSetMutation},
};
use xmtp_proto::xmtp::mls::message_contents::ComponentType;

mod bounds;
mod metadata_reads;

pub(crate) use bounds::{
    check_field_bounds, check_payload_element_bound, has_payload_bound, is_field_bounded,
    registered_type, registry_type_tag,
};
use bounds::{check_update_payload_bounds, resolved_type};
#[cfg(test)]
use metadata_reads::encode_inbox_id_set;
pub use metadata_reads::{
    GroupMetadataReturn, inbox_id_str_to_bytes, read_group_membership_from_dict,
    read_group_metadata_from_dict, read_group_metadata_from_extensions,
    read_super_admin_list_from_dict, read_super_admin_list_from_extensions,
};

/// Longest scalar, collection key, or collection value that an application
/// component, `USER_DISPLAY_NAME`, `GROUP_IMAGE`, or `COMPONENT_REGISTRY` may
/// hold.
pub const MAX_FIELD_ELEMENT_BYTES: usize = 8192;

/// Longest serialized collection snapshot that an application component,
/// `USER_DISPLAY_NAME`, `GROUP_IMAGE`, or `COMPONENT_REGISTRY` may hold.
pub const MAX_FIELD_SNAPSHOT_BYTES: usize = 65536;

/// Errors surfaced by the component_source layer.
///
/// `xmtp_mls` embeds this in `GroupError` via `#[from]` for the
/// AppDataUpdate path.
#[derive(Debug, thiserror::Error)]
pub enum ComponentSourceError {
    /// The component id is outside the well-known XMTP range.
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

    /// A mutation does not match the component type of the component it
    /// targets (e.g. a scalar write to `ADMIN_LIST`).
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

    /// An update would leave a bounded component holding a scalar,
    /// collection element, or collection snapshot longer than its limit
    /// ([`MAX_FIELD_ELEMENT_BYTES`] or [`MAX_FIELD_SNAPSHOT_BYTES`]).
    #[error("component {component_id} holds {len} bytes, over the {max}-byte bound")]
    FieldBoundExceeded {
        /// The component whose new value is too large.
        component_id: ComponentId,
        /// Length of the offending scalar, element, or snapshot.
        len: usize,
        /// The bound it exceeds.
        max: usize,
    },
}

impl ComponentSourceError {
    /// Best-effort `ComponentId` extraction for the variants that carry
    /// one — so error-mapping shims can preserve structured context
    /// across the crate boundary into
    /// [`GroupMutableMetadataError::MalformedComponent`] without
    /// stringifying.
    pub fn component_id(&self) -> Option<ComponentId> {
        match self {
            Self::UnknownComponent(id)
            | Self::NotImplemented(id)
            | Self::ImmutableUpdate(id)
            | Self::MismatchedMutation(id)
            | Self::MalformedComponentValue {
                component_id: id, ..
            }
            | Self::FieldBoundExceeded {
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
            // A COMPONENT_REGISTRY delta that violates the registry's
            // write invariants (reserved/hardcoded/out-of-space id,
            // immutable overwrite, undecodable metadata). Structurally
            // a malformed value for the registry component; the reason
            // string carries the specific violation.
            ComponentTypedError::RegistryMutation(e) => Self::MalformedComponentValue {
                component_id: ComponentId::COMPONENT_REGISTRY,
                reason: e.to_string(),
            },
        }
    }
}

impl From<ComponentSourceError> for GroupMutableMetadataError {
    /// Preserve structure where possible. If the source already wraps a
    /// `GroupMutableMetadataError`, unwrap and return that inner variant
    /// verbatim so callers can
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

/// Hardcoded logical type of a well-known component. Returns `None` for
/// app-range components (`0xC000-0xFEFF`) and for any well-known id that
/// is not yet wired into this match.
pub fn component_type(id: ComponentId) -> Option<ComponentType> {
    match id {
        // Hardcoded registry / list components. ComponentRegistry itself is a
        // TlsMap, but permissions are enforced in code — it never flows
        // through this module.
        ComponentId::COMPONENT_REGISTRY => Some(ComponentType::TlsMapBytesBytes),
        ComponentId::SUPER_ADMIN_LIST => Some(ComponentType::TlsSetInboxId),
        ComponentId::ADMIN_LIST => Some(ComponentType::TlsSetInboxId),

        // GroupMembership — TlsMap<InboxId, bytes>
        ComponentId::GROUP_MEMBERSHIP => Some(ComponentType::TlsMapInboxIdBytes),
        ComponentId::USER_DISPLAY_NAME => Some(ComponentType::TlsMapInboxIdString),

        // GroupMutableMetadata-backed string components.
        ComponentId::GROUP_NAME
        | ComponentId::GROUP_DESCRIPTION
        | ComponentId::GROUP_IMAGE_URL
        | ComponentId::APP_DATA
        | ComponentId::MIN_SUPPORTED_PROTOCOL_VERSION => Some(ComponentType::String),

        // GroupMutableMetadata-backed bytes components.
        ComponentId::MESSAGE_DISAPPEAR_FROM_NS
        | ComponentId::MESSAGE_DISAPPEAR_IN_NS
        | ComponentId::COMMIT_LOG_SIGNER
        | ComponentId::GROUP_IMAGE => Some(ComponentType::Bytes),

        // Immutable metadata (not flowable through AppDataUpdate writes,
        // but we still advertise the type for completeness).
        ComponentId::CONVERSATION_TYPE
        | ComponentId::CREATOR_INBOX_ID
        | ComponentId::ONESHOT_MESSAGE
        | ComponentId::GROUP_ROOT_KEY => Some(ComponentType::Bytes),
        ComponentId::DM_MEMBERS => Some(ComponentType::TlsSetInboxId),

        _ => None,
    }
}

use crate::group_mutable_metadata::METADATA_FIELD_COMPONENT_MAP;

/// Map a [`MetadataField`] string to its corresponding `ComponentId`.
///
/// Returns `None` for unknown field names so this can also be called with a
/// raw string coming from a legacy intent payload.
pub fn metadata_field_to_component_id(field_name: &str) -> Option<ComponentId> {
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
pub fn component_id_to_metadata_field(id: ComponentId) -> Option<MetadataField> {
    METADATA_FIELD_COMPONENT_MAP
        .iter()
        .find(|(_, component_id)| *component_id == id)
        .map(|(field, _)| *field)
}

/// Read the component's current bytes from the OpenMLS AppData dictionary.
pub fn read_component_bytes(
    id: ComponentId,
    extensions: &Extensions<GroupContext>,
) -> Result<Option<&[u8]>, ComponentSourceError> {
    Ok(read_from_app_data_dict_from_extensions(id, extensions))
}

/// Look up the component's bytes in the OpenMLS AppData dictionary.
///
/// The commit validator in `xmtp_mls` pulls the pre-commit stored bytes
/// for a component and threads them into
/// [`expand_app_data_update_to_changes`] as `old_value` — the validator
/// uses that to resolve `RemoveByHash` mutations back to their underlying
/// inbox id. `xmtp_mls`'s `app_data` module also uses it from
/// `process_message_with_app_data`, `stage_app_data_propose_and_commit`,
/// and `AppDataBatch::pending`.
pub fn read_from_app_data_dict(id: ComponentId, mls_group: &OpenMlsGroup) -> Option<&[u8]> {
    read_from_app_data_dict_from_extensions(id, mls_group.extensions())
}

/// Extensions-only counterpart of [`read_from_app_data_dict`] — reads the
/// component's bytes straight from a group's `GroupContext` extensions, with no
/// full `OpenMlsGroup`. openmls keys the dictionary by its own `ComponentId`,
/// which is just a `u16` alias, so `id.as_u16()` unwraps our newtype to the key.
///
/// Borrows the bytes, because a component can be the commit-log signer's
/// private key. A caller that needs an owned copy holds it in `Zeroizing`.
pub fn read_from_app_data_dict_from_extensions(
    id: ComponentId,
    extensions: &Extensions<GroupContext>,
) -> Option<&[u8]> {
    extensions
        .app_data_dictionary()
        .and_then(|ext| ext.dictionary().get(&id.as_u16()))
}

use crate::app_data::typed::ExpandedComponentChange;

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
///
/// The changes are `Zeroizing`: a value can be the commit-log signer's
/// private key.
pub fn expand_app_data_update_to_changes(
    component_id: ComponentId,
    operation: &AppDataUpdateOperation,
    old_value: Option<&[u8]>,
    registry: &ComponentRegistry,
) -> Result<Zeroizing<Vec<ExpandedComponentChange>>, ComponentSourceError> {
    expand_app_data_update_to_changes_with_validator(
        component_id,
        operation,
        old_value,
        &mut ComponentWriteValidator::new(registry),
    )
}

/// [`expand_app_data_update_to_changes`], resolving an unknown component's
/// type from the registry entry that `validator` decodes once.
pub fn expand_app_data_update_to_changes_with_validator(
    component_id: ComponentId,
    operation: &AppDataUpdateOperation,
    old_value: Option<&[u8]>,
    validator: &mut ComponentWriteValidator<'_>,
) -> Result<Zeroizing<Vec<ExpandedComponentChange>>, ComponentSourceError> {
    if let Some(component) = lookup_component(component_id) {
        return component
            .expand_to_changes(operation, old_value)
            .map(Zeroizing::new)
            .map_err(Into::into);
    }

    // No per-id [`Component`] impl on this client. The closed type
    // universe (7 variants) means every shape — including `TlsSet` /
    // `TlsMap` deltas — surfaces a proper per-element change list to the
    // validator. Old and new clients converge on the same dict state for
    // the same wire bytes.
    let ty = resolved_type(component_id, validator)?;
    let changes = Zeroizing::new(expand_to_changes_for_type(
        component_id,
        ty,
        operation,
        old_value,
    )?);
    for value in changes.iter().filter_map(|change| change.value.as_deref()) {
        check_group_root_length(component_id, value)?;
    }
    Ok(changes)
}

/// Reject a `GROUP_ROOT_KEY` value that is not 32 bytes, the length the
/// well-known component table gives the group root.
// implements: META-010
pub(crate) fn check_group_root_length(
    id: ComponentId,
    value: &[u8],
) -> Result<(), ComponentSourceError> {
    if id == ComponentId::GROUP_ROOT_KEY && value.len() != GROUP_ROOT_BYTES {
        return Err(ComponentSourceError::MalformedComponentValue {
            component_id: id,
            reason: format!(
                "a group root is {GROUP_ROOT_BYTES} bytes, not {}",
                value.len()
            ),
        });
    }
    Ok(())
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
///
/// A bounded component's payload is held to the element bound before
/// anything else, so the result does not depend on the state it applies
/// to; the new value is then held to the snapshot bound.
///
/// The new value is `Zeroizing`: it can be the commit-log signer's private
/// key.
pub fn apply_app_data_update_payload(
    id: ComponentId,
    payload: &[u8],
    old_value: Option<&[u8]>,
    registry: &ComponentRegistry,
) -> Result<Zeroizing<Vec<u8>>, ComponentSourceError> {
    apply_app_data_update_payload_with_validator(
        id,
        payload,
        old_value,
        &mut ComponentWriteValidator::new(registry),
    )
}

/// [`apply_app_data_update_payload`], resolving the component's type from
/// the registry entry that `validator` decodes once.
pub fn apply_app_data_update_payload_with_validator(
    id: ComponentId,
    payload: &[u8],
    old_value: Option<&[u8]>,
    validator: &mut ComponentWriteValidator<'_>,
) -> Result<Zeroizing<Vec<u8>>, ComponentSourceError> {
    check_update_payload_bounds(id, payload, validator)?;
    // Immutability gate. Reject only on overwrite — a fresh insert
    // (no prior value) is the bootstrap commit's first write of an
    // immutable seed and must succeed for honest receivers to reach
    // the migrated state. Steady-state immutables always have a prior
    // (inserted at bootstrap), so a Byzantine peer trying to mutate
    // them post-bootstrap still hits this branch and gets rejected.
    if id.is_immutable() && old_value.is_some() {
        return Err(ComponentSourceError::ImmutableUpdate(id));
    }

    // A per-id `Component` impl handles the well-known mutable components
    // with a typed decoder; every other id dispatches on its resolved type.
    let new_value = Zeroizing::new(match lookup_component(id) {
        Some(component) => component.apply_update_payload(payload, old_value)?,
        None => {
            apply_update_payload_for_type(id, resolved_type(id, validator)?, payload, old_value)?
        }
    });
    if is_field_bounded(id) {
        check_field_bounds(id, resolved_type(id, validator)?, &new_value)?;
    }
    check_group_root_length(id, &new_value)?;
    Ok(new_value)
}

/// Overlay AppData-dict component values onto a base [`GroupMutableMetadata`].
///
/// Wire formats (must match what the sender emits via
/// [`apply_app_data_update_payload`]):
/// - Bytes components: raw UTF-8 string bytes.
/// - `ADMIN_LIST` / `SUPER_ADMIN_LIST`: TLS-serialized `TlsSet<InboxId>`,
///   each id hex-encoded back to string form.
///
/// ## Independence from `COMPONENT_REGISTRY` parseability
///
/// This function reads metadata field entries directly from the dict and
/// **never** loads or validates the `COMPONENT_REGISTRY` payload — it
/// does not use the `COMPONENT_REGISTRY` payload as a gate. So a malformed
/// `COMPONENT_REGISTRY` blob does NOT cause
/// metadata reads to drop authoritative data: as long as the individual
/// metadata field bytes (`GROUP_NAME`, `ADMIN_LIST`, …) decode
/// correctly, they round-trip into the returned GMM. Registry corruption
/// is surfaced loudly on the *write* paths instead — the sender gate in
/// `mls_sync.rs` and the commit validator in `validated_commit.rs` both
/// call `xmtp_mls`'s `load_component_registry` and propagate decode errors
/// — so a corrupt registry blocks state changes without making readable
/// data unreachable. See
/// `merge_with_malformed_registry_returns_valid_field` for the test
/// that pins this invariant.
pub fn merge_app_data_into_mutable_metadata(
    base: &mut GroupMutableMetadata,
    mls_group: &OpenMlsGroup,
) -> Result<(), ComponentSourceError> {
    merge_app_data_into_mutable_metadata_from_extensions(base, mls_group.extensions())
}

/// Extract [`GroupMutableMetadata`] from the AppData dictionary.
pub fn extract_group_mutable_metadata_capability_aware(
    mls_group: &OpenMlsGroup,
) -> Result<GroupMutableMetadata, ComponentSourceError> {
    let mut base =
        GroupMutableMetadata::new(std::collections::HashMap::new(), Vec::new(), Vec::new());
    merge_app_data_into_mutable_metadata(&mut base, mls_group)?;
    Ok(base)
}

/// Same as [`extract_group_mutable_metadata_capability_aware`], but driven from
/// the group's `GroupContext` extensions alone — no full `OpenMlsGroup::load`.
/// The mutable metadata lives entirely in the context extensions, so a single
/// `StorageProvider::group_context` read (one KV round-trip, no ratchet tree)
/// is all this needs.
pub fn extract_group_mutable_metadata_capability_aware_from_extensions(
    extensions: &Extensions<GroupContext>,
) -> Result<GroupMutableMetadata, ComponentSourceError> {
    let mut base =
        GroupMutableMetadata::new(std::collections::HashMap::new(), Vec::new(), Vec::new());
    merge_app_data_into_mutable_metadata_from_extensions(&mut base, extensions)?;
    Ok(base)
}

/// Extensions-only variant of [`merge_app_data_into_mutable_metadata`].
pub fn merge_app_data_into_mutable_metadata_from_extensions(
    base: &mut GroupMutableMetadata,
    extensions: &openmls::extensions::Extensions<openmls::group::GroupContext>,
) -> Result<(), ComponentSourceError> {
    // The merge body lives in `xmtp_mls_group_state` so crates below
    // `xmtp_mls` in the dependency graph (the archive exporter) can
    // reuse it. Map the per-component error back to
    // `MalformedComponentValue` so this function's error shape (which
    // callers and tests match on, and `component_id()` extracts from)
    // is unchanged by the move.
    crate::group_mutable_metadata::merge_dict_into_mutable_metadata(base, extensions).map_err(|e| {
        match e {
            GroupMutableMetadataError::MalformedComponent {
                component_id: Some(component_id),
                reason,
            } => ComponentSourceError::MalformedComponentValue {
                component_id,
                reason,
            },
            other => ComponentSourceError::GroupMutableMetadata(other),
        }
    })
}

#[cfg(test)]
mod tests;
