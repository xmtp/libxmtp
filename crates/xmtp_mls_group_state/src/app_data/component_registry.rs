use prost::Message;
use tls_codec::{Deserialize, Serialize, VLBytes};
use xmtp_mls_tls_collections::tls_map::TlsMap;
use xmtp_proto::xmtp::mls::message_contents::{
    ComponentMetadata, ComponentPermissions, ComponentType, MetadataPolicy as MetadataPolicyProto,
    metadata_policy::{Kind as MetadataPolicyKind, MetadataBasePolicy},
};

use super::component_id::ComponentId;

/// The operation being performed on a component.
///
/// Each variant maps to one of the policy fields in
/// [`ComponentPermissions`](xmtp_proto::xmtp::mls::message_contents::ComponentPermissions):
/// `Insert` → `insert_policy`, `Update` → `update_policy`, `Delete` → `delete_policy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ComponentOp {
    /// Creating a new value (component does not yet exist).
    Insert,
    /// Modifying an existing value.
    Update,
    /// Removing a value.
    Delete,
}

impl std::fmt::Display for ComponentOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ComponentOp::Insert => write!(f, "insert"),
            ComponentOp::Update => write!(f, "update"),
            ComponentOp::Delete => write!(f, "delete"),
        }
    }
}

/// Helper to construct a [`ComponentMetadata`] from permissions and type.
pub fn new_component_metadata(
    permissions: ComponentPermissions,
    component_type: ComponentType,
) -> ComponentMetadata {
    ComponentMetadata {
        permissions: Some(permissions),
        component_type: component_type as i32,
        external_committer_permissions: None,
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ComponentRegistryError {
    #[error("component ID {0} is not in the component ID space")]
    InvalidComponentId(ComponentId),
    #[error("component ID {0} is in the reserved range")]
    ReservedRange(ComponentId),
    #[error("immutable component {0} cannot be modified after initial insert")]
    ImmutableComponent(ComponentId),
    #[error("hardcoded component {0} cannot be removed")]
    HardcodedComponent(ComponentId),
    #[error("component {0} not found")]
    NotFound(ComponentId),
    #[error("component {0} metadata is missing the permissions field")]
    MissingPermissions(ComponentId),
    #[error("component {0} metadata is missing the {1} policy field")]
    MissingPolicyField(ComponentId, ComponentOp),
    #[error("decode error for component {component_id}: {source}")]
    DecodeError {
        component_id: ComponentId,
        #[source]
        source: prost::DecodeError,
    },
    #[error("tls codec error: {0}")]
    TlsCodecError(#[from] tls_codec::Error),
    #[error("constrained component {0} requires AllowIfAdmin or AllowIfSuperAdmin policies")]
    ConstrainedPolicyViolation(ComponentId),
}

/// A component registry stored as a `TlsMap<ComponentId, VLBytes>` where
/// each value is a protobuf-encoded [`ComponentMetadata`] describing the
/// component's data type and permission policies.
///
/// The registry provides deterministic TLS serialization (sorted by
/// ComponentId) and enforces that hardcoded and reserved component IDs
/// cannot be modified through this map (their permissions are enforced in
/// code).
///
/// Stored at well-known component ID `0x8000` (`ComponentId::COMPONENT_REGISTRY`).
///
/// ## One raw map, a validated view
///
/// The map holds every entry exactly as it was read, byte for byte —
/// including entries this build cannot validate. Validation is applied
/// lazily on read: [`get`](Self::get), [`iter`](Self::iter),
/// [`contains`](Self::contains), and [`len`](Self::len) present only
/// *recognized* entries (those that pass
/// [`validate_entry`](Self::validate_entry)). An entry that fails
/// validation is invisible to all of them, so a write against it falls to
/// the deny-by-default `NoRegistryEntry` policy verdict. The raw bytes
/// still surface through [`to_bytes`](Self::to_bytes) (verbatim) and
/// [`unrecognized_ids`](Self::unrecognized_ids) (a diagnostic).
///
/// Two invariants motivate carrying bytes we can't read:
///
/// - **Never fork by dropping bytes.** The registry lives inside the
///   `app_data_dictionary` group-context extension; every member must
///   compute byte-identical dict bytes or the group splits. A committer
///   reads the dict, changes one component, and re-emits the whole thing,
///   so it must round-trip untouched entries exactly — and prost does not
///   preserve unknown proto fields across a decode/re-encode, which is why
///   we keep raw `VLBytes` and never re-serialize an entry we didn't
///   author.
/// - **Never brick on state we didn't write.** A single entry left by a
///   newer protocol version, or by a historically buggy writer (a poisoned
///   group), must degrade to "that one component is unwritable here," never
///   to "no commit on this group validates again." Rejecting the whole
///   snapshot would do the latter, because the registry is decoded on the
///   validation path of *every* commit.
///
/// Tolerance is a read-side backstop, not an enforcement hole: new entries
/// still cannot *enter* a group's registry unvalidated (the steady-state
/// wire path validates per-mutation and the bootstrap validator validates
/// each entry of the initial delta).
#[derive(Debug, Clone, PartialEq)]
pub struct ComponentRegistry {
    inner: TlsMap<ComponentId, VLBytes>,
}

impl ComponentRegistry {
    pub fn new() -> Self {
        Self {
            inner: TlsMap::new(),
        }
    }

    /// Get the metadata for a *recognized* component.
    ///
    /// Returns `Ok(None)` both when no entry exists and when an entry exists
    /// but fails [`validate_entry`](Self::validate_entry) — an unreadable
    /// entry is deny-by-default, indistinguishable from absent, so callers
    /// never have to tell the two apart. The `Result` is retained for API
    /// stability; this method does not currently produce an error.
    pub fn get(
        &self,
        id: &ComponentId,
    ) -> Result<Option<ComponentMetadata>, ComponentRegistryError> {
        Ok(self
            .inner
            .get(id)
            .and_then(|raw| Self::decode_recognized(*id, raw.as_slice()).ok()))
    }

    /// Register or update a component's metadata.
    ///
    /// For mutable components in the registry, this silently overwrites any
    /// existing entry — there is no audit log here because the audit trail
    /// lives in the MLS commit history that produced the change.
    ///
    /// A valid write to a mutable id that currently holds an unrecognized
    /// entry repairs it — the raw bytes are overwritten, so `to_bytes`
    /// serializes the repaired value rather than resurrecting the broken
    /// bytes.
    ///
    /// Rejects invalid IDs, IDs in the reserved range, hardcoded components
    /// (whose permissions are enforced in code, not metadata), immutable
    /// components that already hold any raw entry (write-once semantics),
    /// metadata missing required fields, and constrained components with
    /// invalid policy values.
    pub fn set(
        &mut self,
        id: ComponentId,
        meta: ComponentMetadata,
    ) -> Result<(), ComponentRegistryError> {
        self.validate_modifiable(&id)?;
        Self::validate_metadata(&id, &meta)?;
        let bytes = VLBytes::new(meta.encode_to_vec());
        self.inner.set(id, bytes);
        Ok(())
    }

    /// Remove a component from the registry.
    ///
    /// Rejects everything [`set`](Self::set) rejects (invalid IDs, reserved,
    /// hardcoded, and overwrite of a write-once-immutable entry). Removing a
    /// modifiable id drops its bytes whether the entry was recognized or an
    /// unrecognized one preserved by [`from_bytes`](Self::from_bytes)
    /// (repair-by-delete).
    pub fn remove(&mut self, id: &ComponentId) -> Result<(), ComponentRegistryError> {
        self.validate_modifiable(id)?;
        self.inner
            .remove(id)
            .map_err(|_| ComponentRegistryError::NotFound(*id))?;
        Ok(())
    }

    /// Returns true if the registry contains a *recognized* entry for the
    /// given component. An unrecognized (preserved-but-invalid) entry
    /// reports `false`.
    pub fn contains(&self, id: &ComponentId) -> bool {
        self.inner
            .get(id)
            .is_some_and(|raw| Self::decode_recognized(*id, raw.as_slice()).is_ok())
    }

    /// Returns true if the registry holds any bytes for `id`, recognized
    /// or not. Reconciliation keys off this rather than
    /// [`contains`](Self::contains): an entry this build cannot read is
    /// still the group's entry and must not be replaced from a local copy.
    // implements: META-067
    pub fn contains_raw(&self, id: &ComponentId) -> bool {
        self.inner.contains_key(id)
    }

    /// Returns the number of *recognized* entries. Unrecognized entries
    /// preserved by [`from_bytes`](Self::from_bytes) are not counted.
    pub fn len(&self) -> usize {
        self.iter().count()
    }

    /// Returns true if the registry has no recognized entries. A registry
    /// carrying only unrecognized entries still reports empty.
    pub fn is_empty(&self) -> bool {
        self.iter().next().is_none()
    }

    /// Iterate over the *recognized* component IDs and their decoded
    /// metadata, in ComponentId order. Unrecognized entries preserved by
    /// [`from_bytes`](Self::from_bytes) are skipped — writes against them
    /// fall to deny-by-default — and are surfaced via
    /// [`unrecognized_ids`](Self::unrecognized_ids) instead.
    ///
    /// Each item is a `Result` for API stability; a recognized entry always
    /// decodes, so this only ever yields `Ok`.
    pub fn iter(
        &self,
    ) -> impl Iterator<Item = Result<(ComponentId, ComponentMetadata), ComponentRegistryError>> + '_
    {
        self.inner.iter().filter_map(|(&id, raw)| {
            Self::decode_recognized(id, raw.as_slice())
                .ok()
                .map(|meta| Ok((id, meta)))
        })
    }

    /// Serialize the registry as the **dict-storage format**: the raw
    /// `TlsMap<ComponentId, VLBytes>` snapshot, verbatim. Because the map is
    /// stored exactly as read (recognized and unrecognized entries alike), a
    /// load → store round-trip is byte-identical and never drops data
    /// another (possibly newer) client wrote.
    ///
    /// Wire-format encoding (a `TlsMapDelta` describing changes) is done
    /// piecemeal at the call site — there is no whole-registry wire encoder,
    /// because every steady-state update emits only the few entries it
    /// touches. Group creation stores the whole registry with this snapshot
    /// encoding (see [`crate::app_data::creation::initial_dictionary`]).
    // implements: META-016
    pub fn to_bytes(&self) -> Result<Vec<u8>, ComponentRegistryError> {
        Ok(self.inner.tls_serialize_detached()?)
    }

    /// A registry holding exactly `entries`, unvalidated, for registries this
    /// crate defines as data.
    pub(crate) fn from_raw_entries(
        entries: impl IntoIterator<Item = (ComponentId, Vec<u8>)>,
    ) -> Self {
        Self {
            inner: entries
                .into_iter()
                .map(|(id, bytes)| (id, VLBytes::new(bytes)))
                .collect(),
        }
    }

    /// Iterate over every entry exactly as stored, recognized or not, in
    /// ComponentId order. Compare entries through this, not
    /// [`iter`](Self::iter): decoding drops unknown protobuf fields, which
    /// the stored bytes keep.
    pub fn raw_entries(&self) -> impl Iterator<Item = (ComponentId, &[u8])> + '_ {
        self.inner.iter().map(|(&id, raw)| (id, raw.as_slice()))
    }

    /// Deserialize a component registry from its **dict-storage format**: a
    /// raw `TlsMap<ComponentId, VLBytes>` snapshot.
    ///
    /// Fails only when the outer `TlsMap` doesn't decode (truncated /
    /// non-canonical bytes). Individual entries are *not* validated here —
    /// they're kept raw and validated lazily on read, so an entry from a
    /// newer protocol version (or a historical invalid entry) is preserved
    /// rather than making every future commit on the group unvalidatable.
    /// Callers that care can inspect
    /// [`unrecognized_ids`](Self::unrecognized_ids) and log.
    ///
    /// New entries cannot *enter* a group's registry unvalidated: the
    /// steady-state wire path validates per-mutation
    /// (`ComponentRegistryComponent::apply_update_payload` /
    /// `expand_to_changes`) and the bootstrap validator validates each entry
    /// of the initial delta. Tolerance here is the read-side backstop, not
    /// the enforcement point.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ComponentRegistryError> {
        let inner = TlsMap::<ComponentId, VLBytes>::tls_deserialize_exact(bytes)?;
        Ok(Self { inner })
    }

    /// Component ids of entries [`from_bytes`](Self::from_bytes) preserved
    /// but that could not validate. Empty in the overwhelmingly common case;
    /// non-empty means the dict was written by a newer protocol version (or
    /// carries a historical invalid entry) and is worth a log line at the
    /// load site.
    pub fn unrecognized_ids(&self) -> impl Iterator<Item = ComponentId> + '_ {
        self.inner.iter().filter_map(|(&id, raw)| {
            Self::decode_recognized(id, raw.as_slice())
                .is_err()
                .then_some(id)
        })
    }

    /// Validate one `(ComponentId, raw bytes)` entry against the registry's
    /// invariants — the single definition of "recognized." Used by the
    /// wire-decode path in the bootstrap validator (which walks a
    /// `TlsMapDelta`'s mutations directly so it can surface the `Insert`-only
    /// check before per-entry validation) and, internally, by every read
    /// method.
    pub fn validate_entry(id: ComponentId, raw: &[u8]) -> Result<(), ComponentRegistryError> {
        Self::decode_recognized(id, raw).map(|_| ())
    }

    /// Decode and fully validate a stored entry, returning its metadata iff
    /// the entry is recognized. This is the one place that decides
    /// visibility: [`get`](Self::get), [`iter`](Self::iter),
    /// [`contains`](Self::contains), [`len`](Self::len),
    /// [`unrecognized_ids`](Self::unrecognized_ids), and
    /// `WorkingSet::registry_entry` are all defined in terms of whether this
    /// returns `Ok`.
    ///
    /// Rejects ids outside the component space, reserved ids, hardcoded ids
    /// (their permissions are enforced in code), values that don't decode as
    /// `ComponentMetadata`, and metadata that is structurally incomplete or
    /// violates a constrained component's policy allowlist.
    // implements: META-014
    pub(crate) fn decode_recognized(
        id: ComponentId,
        raw: &[u8],
    ) -> Result<ComponentMetadata, ComponentRegistryError> {
        if !id.is_in_component_space() {
            return Err(ComponentRegistryError::InvalidComponentId(id));
        }
        if id.is_reserved() {
            return Err(ComponentRegistryError::ReservedRange(id));
        }
        if id.is_hardcoded() {
            return Err(ComponentRegistryError::HardcodedComponent(id));
        }
        #[cfg(any(test, feature = "test-utils"))]
        super::validation::record_work(|work| work.registry_decodes += 1);
        let meta = ComponentMetadata::decode(raw).map_err(|source| {
            ComponentRegistryError::DecodeError {
                component_id: id,
                source,
            }
        })?;
        Self::validate_metadata(&id, &meta)?;
        Ok(meta)
    }

    /// Validate that the registry entry for `id` can be inserted, updated,
    /// or removed.
    ///
    /// Rejects:
    /// - IDs outside the component ID space
    /// - IDs in the reserved range
    /// - Hardcoded IDs (their permissions are enforced in code; allowing
    ///   metadata entries here would create a silent disagreement between
    ///   what the registry says and what `validate_component_write` actually
    ///   enforces)
    /// - Immutable IDs that already hold any raw entry (write-once). An
    ///   entry this build cannot read may be a newer client's valid one, so
    ///   write-once covers it too.
    // implements: META-015
    fn validate_modifiable(&self, id: &ComponentId) -> Result<(), ComponentRegistryError> {
        if !id.is_in_component_space() {
            return Err(ComponentRegistryError::InvalidComponentId(*id));
        }
        if id.is_reserved() {
            return Err(ComponentRegistryError::ReservedRange(*id));
        }
        if id.is_hardcoded() {
            return Err(ComponentRegistryError::HardcodedComponent(*id));
        }
        if id.is_immutable() && self.contains_raw(id) {
            return Err(ComponentRegistryError::ImmutableComponent(*id));
        }
        Ok(())
    }

    /// Validate that the metadata for a component is structurally complete
    /// and (for constrained components) uses allowed policy values.
    ///
    /// Every component must have:
    /// - `permissions` set to `Some`
    /// - All three policy fields (`insert_policy`, `update_policy`,
    ///   `delete_policy`) set to `Some`
    ///
    /// Constrained components (e.g. `ADMIN_LIST`) additionally require each
    /// policy to be the base policy `Deny`, `AllowIfAdmin` (admin or super
    /// admin), or `AllowIfSuperAdmin` (super admin only). Combinator policies
    /// (`AndCondition` / `AnyCondition`) are rejected for constrained
    /// components.
    fn validate_metadata(
        id: &ComponentId,
        meta: &ComponentMetadata,
    ) -> Result<(), ComponentRegistryError> {
        let perms = meta
            .permissions
            .as_ref()
            .ok_or(ComponentRegistryError::MissingPermissions(*id))?;

        for (op, policy) in [
            (ComponentOp::Insert, &perms.insert_policy),
            (ComponentOp::Update, &perms.update_policy),
            (ComponentOp::Delete, &perms.delete_policy),
        ] {
            let p = policy
                .as_ref()
                .ok_or(ComponentRegistryError::MissingPolicyField(*id, op))?;

            if id.is_constrained() && !Self::is_admin_or_super_admin_or_deny_policy(p) {
                return Err(ComponentRegistryError::ConstrainedPolicyViolation(*id));
            }
        }
        Ok(())
    }

    /// Returns true if the policy is the base policy `Deny`,
    /// `AllowIfAdmin` (admin or super admin), or `AllowIfSuperAdmin`
    /// (super admin only).
    ///
    /// All variants are matched explicitly with no catch-all so that adding a
    /// new `MetadataPolicyKind` variant in the proto forces a compile error
    /// here, requiring an explicit decision about how it interacts with
    /// constrained components.
    fn is_admin_or_super_admin_or_deny_policy(policy: &MetadataPolicyProto) -> bool {
        match &policy.kind {
            Some(MetadataPolicyKind::Base(base)) => {
                // XIP-81 §2.2.1 lists only AllowIfAdmin and
                // AllowIfSuperAdmin for ADMIN_LIST. The shipped constraint
                // also accepts Deny because PolicySet::new_dm() uses Deny
                // for its admin slots; without it, the DM preset cannot be
                // synthesized into the dictionary.
                *base == MetadataBasePolicy::Deny as i32
                    || *base == MetadataBasePolicy::AllowIfAdmin as i32
                    || *base == MetadataBasePolicy::AllowIfSuperAdmin as i32
            }
            // Combinator policies are explicitly rejected for constrained
            // components — even if every leaf is admin-only, the combinator
            // wrapper itself is not on the constrained-component allowlist.
            Some(MetadataPolicyKind::AndCondition(_)) => false,
            Some(MetadataPolicyKind::AnyCondition(_)) => false,
            None => false,
        }
    }
}

impl Default for ComponentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
