use crate::tls_map::TlsMap;
use prost::Message;
use tls_codec::{Deserialize, Serialize, VLBytes};
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

/// A component registry stored as a `TlsMap<ComponentId, VLBytes>` where each
/// value is a protobuf-encoded [`ComponentMetadata`] describing the
/// component's data type and permission policies.
///
/// The registry provides deterministic TLS serialization (sorted by ComponentId)
/// and enforces that hardcoded and reserved component IDs cannot be modified
/// through this map (their permissions are enforced in code).
///
/// Stored at well-known component ID `0x8000` (`ComponentId::COMPONENT_REGISTRY`).
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

    /// Get the metadata for a component, decoding from protobuf bytes.
    ///
    /// In practice the decode should never fail after a successful
    /// [`from_bytes`](Self::from_bytes) or [`set`](Self::set) because both
    /// validate the bytes are decodable. Returning a `Result` here is
    /// defense in depth against in-memory corruption or future bugs.
    pub fn get(
        &self,
        id: &ComponentId,
    ) -> Result<Option<ComponentMetadata>, ComponentRegistryError> {
        match self.inner.get(id) {
            Some(bytes) => {
                let meta = ComponentMetadata::decode(bytes.as_slice()).map_err(|source| {
                    ComponentRegistryError::DecodeError {
                        component_id: *id,
                        source,
                    }
                })?;
                Ok(Some(meta))
            }
            None => Ok(None),
        }
    }

    /// Register or update a component's metadata.
    ///
    /// For mutable components in the registry, this silently overwrites any
    /// existing entry — there is no audit log here because the audit trail
    /// lives in the MLS commit history that produced the change.
    ///
    /// Rejects invalid IDs, IDs in the reserved range, hardcoded components
    /// (whose permissions are enforced in code, not metadata), immutable
    /// components that already have a registry entry (write-once semantics),
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
    /// hardcoded, write-once-immutable). Hardcoded components can never be
    /// in the registry to begin with, so this is just defense in depth.
    pub fn remove(&mut self, id: &ComponentId) -> Result<(), ComponentRegistryError> {
        self.validate_modifiable(id)?;
        self.inner
            .remove(id)
            .map_err(|_| ComponentRegistryError::NotFound(*id))?;
        Ok(())
    }

    /// Returns true if the registry contains metadata for the given component.
    pub fn contains(&self, id: &ComponentId) -> bool {
        self.inner.contains_key(id)
    }

    /// Returns the number of entries in the registry.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns true if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Iterate over component IDs and their decoded metadata.
    ///
    /// Each item is a `Result` so that callers can decide how to handle a
    /// corrupt entry. We never silently drop entries — a permission map is
    /// security-critical and a missing entry would be a security bug.
    pub fn iter(
        &self,
    ) -> impl Iterator<Item = Result<(ComponentId, ComponentMetadata), ComponentRegistryError>> + '_
    {
        self.inner.iter().map(|(&id, bytes)| {
            let meta = ComponentMetadata::decode(bytes.as_slice()).map_err(|source| {
                ComponentRegistryError::DecodeError {
                    component_id: id,
                    source,
                }
            })?;
            Ok((id, meta))
        })
    }

    /// Serialize the entire registry to TLS-encoded bytes.
    pub fn to_bytes(&self) -> Result<Vec<u8>, ComponentRegistryError> {
        Ok(self.inner.tls_serialize_detached()?)
    }

    /// Deserialize a component registry from TLS-encoded bytes.
    ///
    /// Validates that every key is in the component ID space, not reserved,
    /// and not hardcoded, and that every value decodes as a valid
    /// [`ComponentMetadata`] — peers cannot send us a registry with
    /// structurally invalid keys or malformed values, nor can they smuggle
    /// in entries for hardcoded components (whose permissions are enforced
    /// in code). The decoded values are discarded; we keep the original raw
    /// bytes so re-serialization is byte-identical to what was received.
    ///
    /// Immutability is intentionally not enforced here because it's a
    /// write-time concern, not a wire-format invariant.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ComponentRegistryError> {
        let inner: TlsMap<ComponentId, VLBytes> = TlsMap::tls_deserialize_exact(bytes)?;
        for (id, raw) in inner.iter() {
            if !id.is_in_component_space() {
                return Err(ComponentRegistryError::InvalidComponentId(*id));
            }
            if id.is_reserved() {
                return Err(ComponentRegistryError::ReservedRange(*id));
            }
            if id.is_hardcoded() {
                return Err(ComponentRegistryError::HardcodedComponent(*id));
            }
            // Eagerly verify the value decodes AND is structurally valid —
            // fail fast at the wire boundary so callers don't get a partial
            // failure mid-iteration.
            let meta = ComponentMetadata::decode(raw.as_slice()).map_err(|source| {
                ComponentRegistryError::DecodeError {
                    component_id: *id,
                    source,
                }
            })?;
            Self::validate_metadata(id, &meta)?;
        }
        Ok(Self { inner })
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
    /// - IDs in immutable ranges that already have an entry (write-once)
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
        if id.is_immutable() && self.inner.contains_key(id) {
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
    /// policy to be the base policy `AllowIfAdmin` (admin or super admin) or
    /// `AllowIfSuperAdmin` (super admin only). Combinator policies
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

            if id.is_constrained() && !Self::is_admin_or_super_admin_policy(p) {
                return Err(ComponentRegistryError::ConstrainedPolicyViolation(*id));
            }
        }
        Ok(())
    }

    /// Returns true if the policy is the base policy `AllowIfAdmin`
    /// (admin or super admin) or `AllowIfSuperAdmin` (super admin only).
    ///
    /// All variants are matched explicitly with no catch-all so that adding a
    /// new `MetadataPolicyKind` variant in the proto forces a compile error
    /// here, requiring an explicit decision about how it interacts with
    /// constrained components.
    fn is_admin_or_super_admin_policy(policy: &MetadataPolicyProto) -> bool {
        match &policy.kind {
            Some(MetadataPolicyKind::Base(base)) => {
                *base == MetadataBasePolicy::AllowIfAdmin as i32
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
impl ComponentRegistry {
    /// Test-only constructor that bypasses all validation.
    ///
    /// Used to exercise the defense-in-depth code paths in `get` and `iter`
    /// that handle corrupt in-memory state — paths that the public API can
    /// never reach because both `set` and `from_bytes` validate eagerly.
    fn from_inner_for_test(inner: TlsMap<ComponentId, VLBytes>) -> Self {
        Self { inner }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_data::component_permissions::component_permissions;
    use xmtp_proto::xmtp::mls::message_contents::metadata_policy::{AndCondition, AnyCondition};

    fn allow() -> MetadataPolicyProto {
        MetadataPolicyProto {
            kind: Some(MetadataPolicyKind::Base(MetadataBasePolicy::Allow as i32)),
        }
    }

    fn deny() -> MetadataPolicyProto {
        MetadataPolicyProto {
            kind: Some(MetadataPolicyKind::Base(MetadataBasePolicy::Deny as i32)),
        }
    }

    fn admin_only() -> MetadataPolicyProto {
        MetadataPolicyProto {
            kind: Some(MetadataPolicyKind::Base(
                MetadataBasePolicy::AllowIfAdmin as i32,
            )),
        }
    }

    /// An `AndCondition` whose leaves are all `AllowIfAdmin`. The combinator
    /// wrapper itself must still be rejected for constrained components,
    /// regardless of how admin-y its contents are.
    fn and_condition_admin_only() -> MetadataPolicyProto {
        MetadataPolicyProto {
            kind: Some(MetadataPolicyKind::AndCondition(AndCondition {
                policies: vec![admin_only(), admin_only()],
            })),
        }
    }

    /// An `AnyCondition` whose leaves are all `AllowIfAdmin`. Same rationale
    /// as `and_condition_admin_only`.
    fn any_condition_admin_only() -> MetadataPolicyProto {
        MetadataPolicyProto {
            kind: Some(MetadataPolicyKind::AnyCondition(AnyCondition {
                policies: vec![admin_only(), admin_only()],
            })),
        }
    }

    fn sample_meta() -> ComponentMetadata {
        new_component_metadata(
            component_permissions()
                .insert(allow())
                .update(admin_only())
                .delete(deny())
                .call(),
            ComponentType::Bytes,
        )
    }

    #[xmtp_common::test]
    fn test_set_and_get() {
        let mut reg = ComponentRegistry::new();
        let id = ComponentId::GROUP_NAME;
        reg.set(id, sample_meta()).unwrap();
        let meta = reg.get(&id).unwrap().unwrap();
        assert_eq!(meta, sample_meta());
    }

    #[xmtp_common::test]
    fn test_get_missing_returns_none() {
        let reg = ComponentRegistry::new();
        assert!(reg.get(&ComponentId::GROUP_NAME).unwrap().is_none());
    }

    #[xmtp_common::test]
    fn test_set_overwrites() {
        let mut reg = ComponentRegistry::new();
        let id = ComponentId::GROUP_NAME;
        reg.set(id, sample_meta()).unwrap();

        let new_meta = new_component_metadata(
            component_permissions()
                .insert(deny())
                .update(deny())
                .delete(deny())
                .call(),
            ComponentType::TlsMapBytesBytes,
        );
        reg.set(id, new_meta.clone()).unwrap();

        let got = reg.get(&id).unwrap().unwrap();
        assert_eq!(got, new_meta);
        assert_eq!(reg.len(), 1);
    }

    #[xmtp_common::test]
    fn test_remove() {
        let mut reg = ComponentRegistry::new();
        let id = ComponentId::GROUP_NAME;
        reg.set(id, sample_meta()).unwrap();
        reg.remove(&id).unwrap();
        assert!(!reg.contains(&id));
    }

    #[xmtp_common::test]
    fn test_remove_missing_returns_error() {
        let mut reg = ComponentRegistry::new();
        let result = reg.remove(&ComponentId::GROUP_NAME);
        assert!(result.is_err());
    }

    #[xmtp_common::test]
    fn test_reject_hardcoded_set() {
        // Hardcoded components must NEVER have a registry entry. Their
        // permissions are enforced in code by `validate_component_write`,
        // and a stored entry would create a silent disagreement between the
        // registry and the actual enforcement path.
        let mut reg = ComponentRegistry::new();
        assert!(matches!(
            reg.set(ComponentId::COMPONENT_REGISTRY, sample_meta()),
            Err(ComponentRegistryError::HardcodedComponent(_))
        ));
        assert!(matches!(
            reg.set(ComponentId::SUPER_ADMIN_LIST, sample_meta()),
            Err(ComponentRegistryError::HardcodedComponent(_))
        ));
        assert!(reg.is_empty());
    }

    #[xmtp_common::test]
    fn test_reject_hardcoded_remove() {
        // Even though hardcoded entries should never be in the registry, the
        // remove path still rejects them for defense in depth.
        let mut reg = ComponentRegistry::new();
        assert!(matches!(
            reg.remove(&ComponentId::COMPONENT_REGISTRY),
            Err(ComponentRegistryError::HardcodedComponent(_))
        ));
        assert!(matches!(
            reg.remove(&ComponentId::SUPER_ADMIN_LIST),
            Err(ComponentRegistryError::HardcodedComponent(_))
        ));
    }

    #[xmtp_common::test]
    fn test_admin_list_accepts_admin_or_super_admin_policy() {
        let mut reg = ComponentRegistry::new();
        let meta = new_component_metadata(
            component_permissions()
                .insert(admin_only())
                .update(admin_only())
                .delete(admin_only())
                .call(),
            ComponentType::Bytes,
        );
        assert!(reg.set(ComponentId::ADMIN_LIST, meta).is_ok());
    }

    #[xmtp_common::test]
    fn test_admin_list_accepts_super_admin_only_policy() {
        let mut reg = ComponentRegistry::new();
        let super_admin_only = MetadataPolicyProto {
            kind: Some(MetadataPolicyKind::Base(
                MetadataBasePolicy::AllowIfSuperAdmin as i32,
            )),
        };
        let meta = new_component_metadata(
            component_permissions()
                .insert(super_admin_only.clone())
                .update(super_admin_only.clone())
                .delete(super_admin_only)
                .call(),
            ComponentType::Bytes,
        );
        assert!(reg.set(ComponentId::ADMIN_LIST, meta).is_ok());
    }

    #[xmtp_common::test]
    fn test_admin_list_rejects_allow_policy() {
        let mut reg = ComponentRegistry::new();
        let meta = new_component_metadata(
            component_permissions()
                .insert(allow())
                .update(allow())
                .delete(allow())
                .call(),
            ComponentType::Bytes,
        );
        assert!(matches!(
            reg.set(ComponentId::ADMIN_LIST, meta),
            Err(ComponentRegistryError::ConstrainedPolicyViolation(_))
        ));
    }

    #[xmtp_common::test]
    fn test_admin_list_rejects_deny_policy() {
        let mut reg = ComponentRegistry::new();
        let meta = new_component_metadata(
            component_permissions()
                .insert(admin_only())
                .update(deny())
                .delete(admin_only())
                .call(),
            ComponentType::Bytes,
        );
        assert!(matches!(
            reg.set(ComponentId::ADMIN_LIST, meta),
            Err(ComponentRegistryError::ConstrainedPolicyViolation(_))
        ));
    }

    #[xmtp_common::test]
    fn test_admin_list_rejects_mixed_invalid_policy() {
        let mut reg = ComponentRegistry::new();
        let meta = new_component_metadata(
            component_permissions()
                .insert(admin_only())
                .update(allow())
                .delete(admin_only())
                .call(),
            ComponentType::Bytes,
        );
        assert!(matches!(
            reg.set(ComponentId::ADMIN_LIST, meta),
            Err(ComponentRegistryError::ConstrainedPolicyViolation(_))
        ));
    }

    #[xmtp_common::test]
    fn test_admin_list_rejects_and_condition_policy() {
        // Combinator policies must be rejected for constrained components
        // even when every leaf is admin-only — it's the wrapper itself that's
        // disallowed, not the contents. Locks in the explicit rejection in
        // `is_admin_or_super_admin_policy` so a future refactor of that match
        // can't silently let combinators through.
        let mut reg = ComponentRegistry::new();
        let meta = new_component_metadata(
            component_permissions()
                .insert(and_condition_admin_only())
                .update(admin_only())
                .delete(admin_only())
                .call(),
            ComponentType::Bytes,
        );
        assert!(matches!(
            reg.set(ComponentId::ADMIN_LIST, meta),
            Err(ComponentRegistryError::ConstrainedPolicyViolation(_))
        ));
    }

    #[xmtp_common::test]
    fn test_admin_list_rejects_any_condition_policy() {
        // Same rationale as `test_admin_list_rejects_and_condition_policy`,
        // but exercising the `AnyCondition` branch on a different policy slot
        // so all three slots and both combinator variants are covered across
        // the constrained-component test cluster.
        let mut reg = ComponentRegistry::new();
        let meta = new_component_metadata(
            component_permissions()
                .insert(admin_only())
                .update(any_condition_admin_only())
                .delete(admin_only())
                .call(),
            ComponentType::Bytes,
        );
        assert!(matches!(
            reg.set(ComponentId::ADMIN_LIST, meta),
            Err(ComponentRegistryError::ConstrainedPolicyViolation(_))
        ));
    }

    #[xmtp_common::test]
    fn test_rejects_missing_permissions() {
        let mut reg = ComponentRegistry::new();
        // Construct ComponentMetadata directly (bypassing new_component_metadata)
        // because the helper doesn't allow `permissions: None` — that's exactly
        // the negative case we're testing here.
        let meta = ComponentMetadata {
            permissions: None,
            component_type: ComponentType::Bytes as i32,
        };
        // Applies to ALL components, not just constrained ones.
        assert!(matches!(
            reg.set(ComponentId::GROUP_NAME, meta.clone()),
            Err(ComponentRegistryError::MissingPermissions(_))
        ));
        assert!(matches!(
            reg.set(ComponentId::ADMIN_LIST, meta),
            Err(ComponentRegistryError::MissingPermissions(_))
        ));
    }

    #[xmtp_common::test]
    fn test_rejects_missing_policy_field() {
        let mut reg = ComponentRegistry::new();
        let meta = new_component_metadata(
            ComponentPermissions {
                insert_policy: Some(allow()),
                update_policy: None, // missing
                delete_policy: Some(allow()),
            },
            ComponentType::Bytes,
        );
        // Applies to ALL components, not just constrained ones.
        assert!(matches!(
            reg.set(ComponentId::GROUP_NAME, meta),
            Err(ComponentRegistryError::MissingPolicyField(
                _,
                ComponentOp::Update
            ))
        ));
    }

    #[xmtp_common::test]
    fn test_reject_reserved_set() {
        let mut reg = ComponentRegistry::new();
        assert!(matches!(
            reg.set(ComponentId::new(0xFF00), sample_meta()),
            Err(ComponentRegistryError::ReservedRange(_))
        ));
    }

    #[xmtp_common::test]
    fn test_reject_invalid_id() {
        let mut reg = ComponentRegistry::new();
        assert!(matches!(
            reg.set(ComponentId::new(0x0001), sample_meta()),
            Err(ComponentRegistryError::InvalidComponentId(_))
        ));
    }

    #[xmtp_common::test]
    fn test_app_range_allowed() {
        let mut reg = ComponentRegistry::new();
        let id = ComponentId::new(0xC000);
        reg.set(id, sample_meta()).unwrap();
        assert!(reg.contains(&id));
    }

    #[xmtp_common::test]
    fn test_immutable_first_insert_allowed() {
        // Immutable components can be inserted once.
        let mut reg = ComponentRegistry::new();
        let id = ComponentId::new(0xBE00);
        reg.set(id, sample_meta()).unwrap();
        assert!(reg.contains(&id));
    }

    #[xmtp_common::test]
    fn test_immutable_subsequent_set_rejected() {
        let mut reg = ComponentRegistry::new();
        let id = ComponentId::new(0xBE00);
        reg.set(id, sample_meta()).unwrap();
        // Second set on the same immutable id is rejected.
        assert!(matches!(
            reg.set(id, sample_meta()),
            Err(ComponentRegistryError::ImmutableComponent(_))
        ));
    }

    #[xmtp_common::test]
    fn test_immutable_remove_rejected() {
        let mut reg = ComponentRegistry::new();
        let id = ComponentId::new(0xBE00);
        reg.set(id, sample_meta()).unwrap();
        assert!(matches!(
            reg.remove(&id),
            Err(ComponentRegistryError::ImmutableComponent(_))
        ));
        assert!(reg.contains(&id));
    }

    #[xmtp_common::test]
    fn test_tls_round_trip() {
        let mut reg = ComponentRegistry::new();
        reg.set(ComponentId::GROUP_NAME, sample_meta()).unwrap();
        reg.set(
            ComponentId::GROUP_DESCRIPTION,
            new_component_metadata(
                component_permissions()
                    .insert(deny())
                    .update(deny())
                    .delete(deny())
                    .call(),
                ComponentType::Bytes,
            ),
        )
        .unwrap();

        let bytes = reg.to_bytes().unwrap();
        let restored = ComponentRegistry::from_bytes(&bytes).unwrap();
        assert_eq!(reg, restored);
    }

    #[xmtp_common::test]
    fn test_iter() {
        let mut reg = ComponentRegistry::new();
        reg.set(ComponentId::GROUP_NAME, sample_meta()).unwrap();
        reg.set(ComponentId::GROUP_DESCRIPTION, sample_meta())
            .unwrap();

        let entries: Vec<_> = reg.iter().collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(entries.len(), 2);
        assert!(entries[0].0 < entries[1].0);
    }

    #[xmtp_common::test]
    fn test_empty_registry_round_trip() {
        let reg = ComponentRegistry::new();
        let bytes = reg.to_bytes().unwrap();
        let restored = ComponentRegistry::from_bytes(&bytes).unwrap();
        assert_eq!(reg, restored);
        assert!(restored.is_empty());
    }

    #[xmtp_common::test]
    fn test_from_bytes_rejects_out_of_space_id() {
        // Build a TlsMap directly with an out-of-space key (0x0001) and
        // serialize it, then try to load it as a ComponentRegistry.
        let mut raw: TlsMap<ComponentId, VLBytes> = TlsMap::new();
        raw.set(
            ComponentId::new(0x0001),
            VLBytes::new(sample_meta().encode_to_vec()),
        );
        let bytes = raw.tls_serialize_detached().unwrap();
        let result = ComponentRegistry::from_bytes(&bytes);
        assert!(matches!(
            result,
            Err(ComponentRegistryError::InvalidComponentId(_))
        ));
    }

    #[xmtp_common::test]
    fn test_from_bytes_rejects_reserved_id() {
        let mut raw: TlsMap<ComponentId, VLBytes> = TlsMap::new();
        raw.set(
            ComponentId::new(0xFF50),
            VLBytes::new(sample_meta().encode_to_vec()),
        );
        let bytes = raw.tls_serialize_detached().unwrap();
        let result = ComponentRegistry::from_bytes(&bytes);
        assert!(matches!(
            result,
            Err(ComponentRegistryError::ReservedRange(_))
        ));
    }

    /// Build a TLS-encoded `TlsMap<ComponentId, VLBytes>` containing a single
    /// entry. Bypasses `ComponentRegistry::set` so we can construct payloads
    /// that the public API would refuse to produce.
    fn raw_bytes_with_entry(id: ComponentId, value: Vec<u8>) -> Vec<u8> {
        let mut raw: TlsMap<ComponentId, VLBytes> = TlsMap::new();
        raw.set(id, VLBytes::new(value));
        raw.tls_serialize_detached().unwrap()
    }

    #[xmtp_common::test]
    fn test_from_bytes_rejects_hardcoded_id() {
        // Peers must not be able to smuggle hardcoded entries past the wire
        // boundary, since their permissions are enforced in code.
        let bytes = raw_bytes_with_entry(
            ComponentId::COMPONENT_REGISTRY,
            sample_meta().encode_to_vec(),
        );
        assert!(matches!(
            ComponentRegistry::from_bytes(&bytes),
            Err(ComponentRegistryError::HardcodedComponent(_))
        ));

        let bytes =
            raw_bytes_with_entry(ComponentId::SUPER_ADMIN_LIST, sample_meta().encode_to_vec());
        assert!(matches!(
            ComponentRegistry::from_bytes(&bytes),
            Err(ComponentRegistryError::HardcodedComponent(_))
        ));
    }

    #[xmtp_common::test]
    fn test_from_bytes_rejects_missing_permissions() {
        // A peer ships a metadata entry whose permissions field is None.
        // The set() path catches this; from_bytes() must catch it too.
        let meta = ComponentMetadata {
            permissions: None,
            component_type: ComponentType::Bytes as i32,
        };
        let bytes = raw_bytes_with_entry(ComponentId::GROUP_NAME, meta.encode_to_vec());
        assert!(matches!(
            ComponentRegistry::from_bytes(&bytes),
            Err(ComponentRegistryError::MissingPermissions(_))
        ));
    }

    #[xmtp_common::test]
    fn test_from_bytes_rejects_missing_policy_field() {
        // Permissions present, but one of the three policy fields is missing.
        let meta = new_component_metadata(
            ComponentPermissions {
                insert_policy: Some(allow()),
                update_policy: None,
                delete_policy: Some(allow()),
            },
            ComponentType::Bytes,
        );
        let bytes = raw_bytes_with_entry(ComponentId::GROUP_NAME, meta.encode_to_vec());
        assert!(matches!(
            ComponentRegistry::from_bytes(&bytes),
            Err(ComponentRegistryError::MissingPolicyField(
                _,
                ComponentOp::Update
            ))
        ));
    }

    #[xmtp_common::test]
    fn test_from_bytes_rejects_constrained_violation() {
        // A peer ships an ADMIN_LIST entry with an Allow policy, which
        // violates the constrained-component invariant.
        let meta = new_component_metadata(
            component_permissions()
                .insert(allow())
                .update(allow())
                .delete(allow())
                .call(),
            ComponentType::Bytes,
        );
        let bytes = raw_bytes_with_entry(ComponentId::ADMIN_LIST, meta.encode_to_vec());
        assert!(matches!(
            ComponentRegistry::from_bytes(&bytes),
            Err(ComponentRegistryError::ConstrainedPolicyViolation(_))
        ));
    }

    #[xmtp_common::test]
    fn test_iter_surfaces_decode_error_for_corrupt_value() {
        // The public API (`set`, `from_bytes`) eagerly validates so this
        // can't happen via normal use. We construct a registry directly
        // through the test-only constructor to verify that `iter()` would
        // surface a structured DecodeError if the in-memory state ever did
        // get corrupted (in-memory bit flip, future bug, etc.) instead of
        // panicking or silently dropping the entry.
        let mut inner: TlsMap<ComponentId, VLBytes> = TlsMap::new();
        inner.set(
            ComponentId::GROUP_NAME,
            VLBytes::new(sample_meta().encode_to_vec()),
        );
        inner.set(
            ComponentId::GROUP_DESCRIPTION,
            VLBytes::new(vec![0xFF, 0xFF, 0xFF, 0xFF]),
        );
        let reg = ComponentRegistry::from_inner_for_test(inner);

        let results: Vec<_> = reg.iter().collect();
        assert_eq!(results.len(), 2);
        // The good entry decodes successfully.
        assert!(matches!(
            &results[0],
            Ok((id, _)) if *id == ComponentId::GROUP_NAME
        ));
        // The corrupt entry surfaces a structured DecodeError, not a panic
        // or a silent drop.
        assert!(matches!(
            &results[1],
            Err(ComponentRegistryError::DecodeError { component_id, .. })
                if *component_id == ComponentId::GROUP_DESCRIPTION
        ));
    }

    #[xmtp_common::test]
    fn test_get_surfaces_decode_error_for_corrupt_value() {
        // Same defense-in-depth as `test_iter_surfaces_decode_error_*` but
        // for the single-entry `get` path.
        let mut inner: TlsMap<ComponentId, VLBytes> = TlsMap::new();
        inner.set(
            ComponentId::GROUP_NAME,
            VLBytes::new(vec![0xFF, 0xFF, 0xFF, 0xFF]),
        );
        let reg = ComponentRegistry::from_inner_for_test(inner);
        assert!(matches!(
            reg.get(&ComponentId::GROUP_NAME),
            Err(ComponentRegistryError::DecodeError { .. })
        ));
    }

    #[xmtp_common::test]
    fn test_from_bytes_rejects_malformed_protobuf() {
        // The key is valid but the value bytes are not a parseable
        // ComponentMetadata. This is the wire-boundary path for a peer
        // shipping garbage values.
        let bytes = raw_bytes_with_entry(ComponentId::GROUP_NAME, vec![0xFF, 0xFF, 0xFF, 0xFF]);
        assert!(matches!(
            ComponentRegistry::from_bytes(&bytes),
            Err(ComponentRegistryError::DecodeError { .. })
        ));
    }

    #[xmtp_common::test]
    fn test_preserves_component_type() {
        let mut reg = ComponentRegistry::new();
        let meta = new_component_metadata(
            component_permissions()
                .insert(allow())
                .update(allow())
                .delete(deny())
                .call(),
            ComponentType::TlsMapBytesBytes,
        );
        reg.set(ComponentId::GROUP_MEMBERSHIP, meta).unwrap();

        let got = reg.get(&ComponentId::GROUP_MEMBERSHIP).unwrap().unwrap();
        assert_eq!(got.component_type, ComponentType::TlsMapBytesBytes as i32);
    }
}
