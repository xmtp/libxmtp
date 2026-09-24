//! [`Component`] impls for the two `TlsMap`-shaped components:
//! [`GroupMembershipComponent`] (`GROUP_MEMBERSHIP`, key: [`InboxId`])
//! and [`ComponentRegistryComponent`] (`COMPONENT_REGISTRY`, key:
//! [`ComponentId`]).
//!
//! Both store their value as `TlsMap<K, VLBytes>` where the inner
//! `VLBytes` payload is opaque to this layer:
//!
//! - `GROUP_MEMBERSHIP` value bytes are prost-encoded
//!   [`GroupMembershipEntryV1`](xmtp_proto::xmtp::mls::message_contents::GroupMembershipEntry)
//!   blobs; downstream consumers decode them after reading.
//! - `COMPONENT_REGISTRY` value bytes are prost-encoded
//!   [`ComponentMetadata`](xmtp_proto::xmtp::mls::message_contents::ComponentMetadata)
//!   blobs.
//!
//! `Update` payloads are encoded as
//! [`TlsMapDelta<K, VLBytes>`](crate::tls_map::TlsMapDelta) — single
//! mutation per `AppDataUpdate` proposal at the steady-state path
//! (UpdatePermission inserts/updates one registry entry; group
//! membership updates one inbox at a time).

use openmls::messages::proposals::AppDataUpdateOperation;
use tls_codec::{Deserialize, Serialize, VLBytes};
use xmtp_proto::xmtp::mls::message_contents::ComponentType;

use crate::{
    app_data::{
        component_id::ComponentId,
        component_registry::{ComponentOp, ComponentRegistry, ComponentRegistryError},
        typed::{Component, ComponentTypedError, ExpandedComponentChange},
    },
    inbox_id::InboxId,
    tls_map::{TlsMap, TlsMapDelta, TlsMapMutation},
};

/// Apply a `TlsMapDelta<K, VLBytes>` wire payload over the prior dict
/// bytes (a `TlsMap<K, VLBytes>` snapshot, or `None` if this is the
/// first write — bootstrap, where prior state is empty).
///
/// Returns the new dict bytes: the re-serialized `TlsMap<K, VLBytes>`
/// snapshot. The dict always holds the raw map as state; the wire
/// always carries a delta describing the change. This function is
/// the one boundary that translates between them.
///
/// Generic over the key type so both `InboxId`-keyed and
/// `ComponentId`-keyed maps share the same body. Also reachable from the
/// type-aware fallback in
/// [`super::type_dispatch::apply_update_payload_for_type`] (with
/// `K = VLBytes` for the `TlsMapBytesBytes` shape).
pub(crate) fn apply_tls_map_delta<K>(
    payload: &[u8],
    prior: Option<&[u8]>,
) -> Result<Vec<u8>, ComponentTypedError>
where
    K: tls_codec::Serialize
        + tls_codec::Deserialize
        + tls_codec::Size
        + Ord
        + Eq
        + Clone
        + std::fmt::Debug,
{
    let delta = TlsMapDelta::<K, VLBytes>::tls_deserialize_exact(payload)?;
    let mut map: TlsMap<K, VLBytes> = match prior {
        Some(bytes) => TlsMap::<K, VLBytes>::tls_deserialize_exact(bytes)?,
        None => TlsMap::new(),
    };
    map.apply_delta(delta)?;
    Ok(map.tls_serialize_detached()?)
}

/// Expand a `TlsMap`-shape `AppDataUpdate` proposal into per-mutation
/// `ExpandedComponentChange` entries.
///
/// Convention for `value` field on the emitted changes:
/// - `Insert(_, v)` and `Update(_, v)` carry the new value bytes
///   (`v.as_slice().to_vec()`) — this is what change-aware policies
///   would inspect.
/// - `Delete(k)` carries the key bytes via `K::tls_serialize_detached`,
///   matching the `TlsSet` expand convention where the value field
///   identifies the affected element.
///
/// `prior` is currently unused — Map components don't have a
/// `RemoveByHash` analogue (deletes carry the literal key on the
/// wire). Kept in the signature for trait-shape uniformity.
///
/// **Why no payload validation here.** Beyond the TLS codec check
/// (`tls_deserialize_exact` rejects truncated / oversized payloads
/// and structurally-malformed deltas), this expansion does not
/// validate the per-mutation contents — it just feeds them to the
/// validator's per-element policy check. Authoritative semantic
/// validation happens at apply time inside [`TlsMap::apply_delta`]
/// (e.g. `Update`/`Delete` of an absent key returns `KeyNotFound`,
/// duplicate `Insert` returns the appropriate map error). Validating
/// here too would either duplicate that work or, worse, drift out of
/// sync with the apply-time rules and reject things the apply step
/// would happily accept. Keep it single-source: codec here, semantics
/// at apply.
pub(crate) fn expand_tls_map_changes<K>(
    op: &AppDataUpdateOperation,
    _prior: Option<&[u8]>,
) -> Result<Vec<ExpandedComponentChange>, ComponentTypedError>
where
    K: tls_codec::Serialize + tls_codec::Deserialize + tls_codec::Size,
{
    match op {
        AppDataUpdateOperation::Remove => Ok(vec![ExpandedComponentChange {
            op: ComponentOp::Delete,
            value: None,
        }]),
        AppDataUpdateOperation::Update(payload) => {
            let delta = TlsMapDelta::<K, VLBytes>::tls_deserialize_exact(payload.as_slice())?;
            expand_decoded_tls_map_delta(delta)
        }
    }
}

/// Expansion body shared by [`expand_tls_map_changes`] and the
/// registry-specific [`ComponentRegistryComponent::expand_to_changes`]
/// override (which validates the decoded delta first and must not
/// decode the payload twice).
fn expand_decoded_tls_map_delta<K>(
    delta: TlsMapDelta<K, VLBytes>,
) -> Result<Vec<ExpandedComponentChange>, ComponentTypedError>
where
    K: tls_codec::Serialize + tls_codec::Size,
{
    let mut out = Vec::with_capacity(delta.mutations.len());
    for mutation in delta.mutations {
        match mutation {
            TlsMapMutation::Insert { value, .. } => out.push(ExpandedComponentChange {
                op: ComponentOp::Insert,
                value: Some(value.as_slice().to_vec()),
            }),
            TlsMapMutation::Update { value, .. } => out.push(ExpandedComponentChange {
                op: ComponentOp::Update,
                value: Some(value.as_slice().to_vec()),
            }),
            TlsMapMutation::Delete { key } => {
                let key_bytes = key.tls_serialize_detached()?;
                out.push(ExpandedComponentChange {
                    op: ComponentOp::Delete,
                    value: Some(key_bytes),
                });
            }
        }
    }
    Ok(out)
}

// ============================================================================
// GROUP_MEMBERSHIP — TlsMap<InboxId, VLBytes>
// ============================================================================

/// `Component` impl for the `GROUP_MEMBERSHIP` component.
///
/// The decoded value is a `TlsMap<InboxId, VLBytes>` where each value
/// is the prost-encoded
/// [`GroupMembershipEntryV1`](xmtp_proto::xmtp::mls::message_contents::GroupMembershipEntry)
/// for that member. This impl handles only the wire codec; entry
/// content is decoded by the caller.
pub struct GroupMembershipComponent;

impl Component for GroupMembershipComponent {
    const ID: ComponentId = ComponentId::GROUP_MEMBERSHIP;
    const COMPONENT_TYPE: ComponentType = ComponentType::TlsMapInboxIdBytes;
    type Value = TlsMap<InboxId, VLBytes>;
    // The mutation type is the full wire-level delta. Group
    // membership updates need to be atomic — every installation
    // change for an inbox (additions, removals, sequence-id bumps)
    // must travel as one proposal so receivers apply them together.
    type Mutation = TlsMapDelta<InboxId, VLBytes>;

    fn decode_value(bytes: &[u8]) -> Result<Self::Value, ComponentTypedError> {
        TlsMap::<InboxId, VLBytes>::tls_deserialize_exact(bytes).map_err(Into::into)
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, ComponentTypedError> {
        value.tls_serialize_detached().map_err(Into::into)
    }

    fn encode_mutation(mutation: &Self::Mutation) -> Result<Vec<u8>, ComponentTypedError> {
        mutation.tls_serialize_detached().map_err(Into::into)
    }

    fn apply_update_payload(
        payload: &[u8],
        prior: Option<&[u8]>,
    ) -> Result<Vec<u8>, ComponentTypedError> {
        apply_tls_map_delta::<InboxId>(payload, prior)
    }

    fn expand_to_changes(
        op: &AppDataUpdateOperation,
        prior: Option<&[u8]>,
    ) -> Result<Vec<ExpandedComponentChange>, ComponentTypedError> {
        expand_tls_map_changes::<InboxId>(op, prior)
    }
}

// ============================================================================
// COMPONENT_REGISTRY — TlsMap<ComponentId, VLBytes>
// ============================================================================

/// `Component` impl for the `COMPONENT_REGISTRY` component.
///
/// The decoded value is a `TlsMap<ComponentId, VLBytes>` where each
/// value is the prost-encoded
/// [`ComponentMetadata`](xmtp_proto::xmtp::mls::message_contents::ComponentMetadata)
/// describing one registered component.
///
/// Unlike the other map component, this impl validates every mutation
/// in an incoming delta against the registry's write invariants (see
/// [`validate_registry_delta`]) before applying or expanding it. The
/// registry is load-bearing for *all* app-data validation — every
/// policy lookup and every type-aware dispatch decodes it — so a
/// single invalid entry accepted here would poison every subsequent
/// [`ComponentRegistry::from_bytes`] load for every member of the
/// group.
pub struct ComponentRegistryComponent;

/// Validate every mutation in a `COMPONENT_REGISTRY` delta against the
/// registry's write invariants, mirroring what
/// [`ComponentRegistry::set`] / [`ComponentRegistry::remove`] enforce
/// for local writes:
///
/// - Insert/Update: the entry must pass
///   [`ComponentRegistry::validate_entry`] (id in the component space,
///   not reserved, not hardcoded, metadata decodable and structurally
///   complete), and Updates must not target an immutable-range id
///   (write-once).
/// - Delete: the id must be modifiable at all — deletes of
///   out-of-space / reserved / hardcoded / immutable ids are rejected.
///   The tolerant [`ComponentRegistry::from_bytes`] means a poisoned
///   dict CAN carry entries under such ids (preserved-but-invisible),
///   so this arm is a deliberate policy choice, not dead code:
///   non-modifiable poisoned entries are intentionally NOT
///   wire-repairable via Delete. Deny-by-default already keeps them
///   harmless, and keeping the non-modifiable ranges closed to every
///   wire write beats opening a repair path nobody should need.
///   (Poisoned entries under modifiable ids remain repairable — their
///   Delete passes this check and removes the raw entry at apply.)
///
/// Shared by [`ComponentRegistryComponent::apply_update_payload`] and
/// [`ComponentRegistryComponent::expand_to_changes`], so both call
/// sites judge a delta's mutations identically. Scope caveat: this
/// helper only sees `Update` payloads (the `TlsMapDelta`). The
/// whole-registry `AppDataUpdateOperation::Remove` rejection in
/// `expand_to_changes` is enforced only during commit validation
/// (`ValidatedCommit::from_staged_commit` in `xmtp_mls`) — the apply
/// pipeline (`accumulate_app_data_updates` in
/// `xmtp_mls::groups::app_data`) maps `Remove` straight to a dict
/// removal without consulting this component impl, and does not
/// re-check because both current commit-processing paths validate
/// before applying. The Remove ban is a validator-side guarantee, not
/// an apply-time one.
fn validate_registry_delta(
    delta: &TlsMapDelta<ComponentId, VLBytes>,
) -> Result<(), ComponentTypedError> {
    for mutation in &delta.mutations {
        match mutation {
            TlsMapMutation::Insert { key, value } => {
                ComponentRegistry::validate_entry(*key, value.as_slice())?;
            }
            TlsMapMutation::Update { key, value } => {
                ComponentRegistry::validate_entry(*key, value.as_slice())?;
                if key.is_immutable() {
                    return Err(ComponentRegistryError::ImmutableComponent(*key).into());
                }
            }
            TlsMapMutation::Delete { key } => {
                if !key.is_in_component_space() {
                    return Err(ComponentRegistryError::InvalidComponentId(*key).into());
                }
                if key.is_reserved() {
                    return Err(ComponentRegistryError::ReservedRange(*key).into());
                }
                if key.is_hardcoded() {
                    return Err(ComponentRegistryError::HardcodedComponent(*key).into());
                }
                if key.is_immutable() {
                    return Err(ComponentRegistryError::ImmutableComponent(*key).into());
                }
            }
        }
    }
    Ok(())
}

impl Component for ComponentRegistryComponent {
    const ID: ComponentId = ComponentId::COMPONENT_REGISTRY;
    const COMPONENT_TYPE: ComponentType = ComponentType::TlsMapBytesBytes;
    type Value = TlsMap<ComponentId, VLBytes>;
    // The mutation type is the full wire-level delta so a single
    // proposal can register/update/remove multiple component
    // entries atomically (e.g. bulk-registering custom components).
    type Mutation = TlsMapDelta<ComponentId, VLBytes>;

    fn decode_value(bytes: &[u8]) -> Result<Self::Value, ComponentTypedError> {
        TlsMap::<ComponentId, VLBytes>::tls_deserialize_exact(bytes).map_err(Into::into)
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, ComponentTypedError> {
        value.tls_serialize_detached().map_err(Into::into)
    }

    fn encode_mutation(mutation: &Self::Mutation) -> Result<Vec<u8>, ComponentTypedError> {
        mutation.tls_serialize_detached().map_err(Into::into)
    }

    fn apply_update_payload(
        payload: &[u8],
        prior: Option<&[u8]>,
    ) -> Result<Vec<u8>, ComponentTypedError> {
        let delta = TlsMapDelta::<ComponentId, VLBytes>::tls_deserialize_exact(payload)?;
        validate_registry_delta(&delta)?;
        let mut map = match prior {
            Some(bytes) => TlsMap::<ComponentId, VLBytes>::tls_deserialize_exact(bytes)?,
            None => TlsMap::new(),
        };
        map.apply_delta(delta)?;
        Ok(map.tls_serialize_detached()?)
    }

    fn expand_to_changes(
        op: &AppDataUpdateOperation,
        _prior: Option<&[u8]>,
    ) -> Result<Vec<ExpandedComponentChange>, ComponentTypedError> {
        match op {
            // Removing the entire registry component is never legal —
            // the registry is what makes every other app-data write
            // validatable (deny-by-default keys off its entries), and
            // its absence is also the "unmigrated group" marker. A
            // commit deleting it would strand the group in a
            // validated-but-unreadable state. `HardcodedComponent`
            // is the same verdict `ComponentRegistry::remove` gives
            // for this id.
            AppDataUpdateOperation::Remove => Err(ComponentRegistryError::HardcodedComponent(
                ComponentId::COMPONENT_REGISTRY,
            )
            .into()),
            AppDataUpdateOperation::Update(payload) => {
                let delta =
                    TlsMapDelta::<ComponentId, VLBytes>::tls_deserialize_exact(payload.as_slice())?;
                validate_registry_delta(&delta)?;
                expand_decoded_tls_map_delta(delta)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_data::component_permissions::component_permissions;
    use prost::Message;
    use xmtp_proto::xmtp::mls::message_contents::{
        MetadataPolicy as MetadataPolicyProto,
        metadata_policy::{Kind as MetadataPolicyKind, MetadataBasePolicy},
    };

    fn fixture_inbox_id(seed: u8) -> InboxId {
        let mut bytes = [0u8; 32];
        bytes[0] = seed;
        InboxId::from_bytes(bytes)
    }

    /// Encoded, structurally valid `ComponentMetadata` bytes — registry
    /// deltas are entry-validated on apply/expand, so tests must carry
    /// real metadata, not placeholder strings.
    fn valid_meta_bytes() -> Vec<u8> {
        let allow = MetadataPolicyProto {
            kind: Some(MetadataPolicyKind::Base(MetadataBasePolicy::Allow as i32)),
        };
        crate::app_data::component_registry::new_component_metadata(
            component_permissions()
                .insert(allow.clone())
                .update(allow.clone())
                .delete(allow)
                .call(),
            ComponentType::Bytes,
        )
        .encode_to_vec()
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn group_membership_round_trip_value() {
        let mut map: TlsMap<InboxId, VLBytes> = TlsMap::new();
        map.insert(fixture_inbox_id(1), VLBytes::new(b"member1-blob".to_vec()))
            .unwrap();
        let bytes = GroupMembershipComponent::encode_value(&map).unwrap();
        let decoded = GroupMembershipComponent::decode_value(&bytes).unwrap();
        assert_eq!(decoded.len(), 1);
        assert_eq!(
            decoded.get(&fixture_inbox_id(1)).unwrap().as_slice(),
            b"member1-blob"
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn group_membership_apply_insert_against_empty() {
        let delta = TlsMapDelta::<InboxId, VLBytes>::new()
            .insert(fixture_inbox_id(2), VLBytes::new(b"v".to_vec()));
        let payload = GroupMembershipComponent::encode_mutation(&delta).unwrap();
        let new_bytes = GroupMembershipComponent::apply_update_payload(&payload, None).unwrap();
        let new = GroupMembershipComponent::decode_value(&new_bytes).unwrap();
        assert_eq!(new.len(), 1);
        assert_eq!(new.get(&fixture_inbox_id(2)).unwrap().as_slice(), b"v");
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn group_membership_apply_update_against_existing() {
        let mut prior_map: TlsMap<InboxId, VLBytes> = TlsMap::new();
        prior_map
            .insert(fixture_inbox_id(3), VLBytes::new(b"old".to_vec()))
            .unwrap();
        let prior_bytes = GroupMembershipComponent::encode_value(&prior_map).unwrap();

        let delta = TlsMapDelta::<InboxId, VLBytes>::new()
            .update(fixture_inbox_id(3), VLBytes::new(b"new".to_vec()));
        let payload = GroupMembershipComponent::encode_mutation(&delta).unwrap();
        let new_bytes =
            GroupMembershipComponent::apply_update_payload(&payload, Some(&prior_bytes)).unwrap();
        let new = GroupMembershipComponent::decode_value(&new_bytes).unwrap();
        assert_eq!(new.get(&fixture_inbox_id(3)).unwrap().as_slice(), b"new");
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn group_membership_apply_batched_delta_atomically() {
        // The motivating case for delta-as-mutation: all
        // installation changes for an inbox in one proposal —
        // sequence-id bump on member 1, new member 2, removed
        // member 3 land atomically.
        let mut prior_map: TlsMap<InboxId, VLBytes> = TlsMap::new();
        prior_map
            .insert(fixture_inbox_id(1), VLBytes::new(b"seq=5".to_vec()))
            .unwrap();
        prior_map
            .insert(fixture_inbox_id(3), VLBytes::new(b"to-remove".to_vec()))
            .unwrap();
        let prior_bytes = GroupMembershipComponent::encode_value(&prior_map).unwrap();

        let delta = TlsMapDelta::<InboxId, VLBytes>::new()
            .update(fixture_inbox_id(1), VLBytes::new(b"seq=7".to_vec()))
            .insert(fixture_inbox_id(2), VLBytes::new(b"seq=1".to_vec()))
            .delete(fixture_inbox_id(3));
        let payload = GroupMembershipComponent::encode_mutation(&delta).unwrap();
        let new_bytes =
            GroupMembershipComponent::apply_update_payload(&payload, Some(&prior_bytes)).unwrap();
        let new = GroupMembershipComponent::decode_value(&new_bytes).unwrap();

        assert_eq!(new.len(), 2);
        assert_eq!(new.get(&fixture_inbox_id(1)).unwrap().as_slice(), b"seq=7");
        assert_eq!(new.get(&fixture_inbox_id(2)).unwrap().as_slice(), b"seq=1");
        assert!(new.get(&fixture_inbox_id(3)).is_none());
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn group_membership_expand_insert_carries_value_bytes() {
        let delta = TlsMapDelta::<InboxId, VLBytes>::new()
            .insert(fixture_inbox_id(4), VLBytes::new(b"new-member".to_vec()));
        let payload = GroupMembershipComponent::encode_mutation(&delta).unwrap();
        let op = AppDataUpdateOperation::Update(payload.into());
        let changes = GroupMembershipComponent::expand_to_changes(&op, None).unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].op, ComponentOp::Insert);
        assert_eq!(changes[0].value.as_deref(), Some(&b"new-member"[..]));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn group_membership_expand_delete_carries_key_bytes() {
        let delta = TlsMapDelta::<InboxId, VLBytes>::new().delete(fixture_inbox_id(5));
        let payload = GroupMembershipComponent::encode_mutation(&delta).unwrap();
        let op = AppDataUpdateOperation::Update(payload.into());
        let changes = GroupMembershipComponent::expand_to_changes(&op, None).unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].op, ComponentOp::Delete);
        // The value field carries the TLS-encoded key for context.
        assert!(changes[0].value.is_some());
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn group_membership_expand_batched_yields_one_per_mutation() {
        let delta = TlsMapDelta::<InboxId, VLBytes>::new()
            .update(fixture_inbox_id(1), VLBytes::new(b"v1".to_vec()))
            .insert(fixture_inbox_id(2), VLBytes::new(b"v2".to_vec()))
            .delete(fixture_inbox_id(3));
        let payload = GroupMembershipComponent::encode_mutation(&delta).unwrap();
        let op = AppDataUpdateOperation::Update(payload.into());
        let changes = GroupMembershipComponent::expand_to_changes(&op, None).unwrap();
        assert_eq!(changes.len(), 3);
        assert_eq!(changes[0].op, ComponentOp::Update);
        assert_eq!(changes[1].op, ComponentOp::Insert);
        assert_eq!(changes[2].op, ComponentOp::Delete);
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn component_registry_round_trip() {
        let mut map: TlsMap<ComponentId, VLBytes> = TlsMap::new();
        map.insert(
            ComponentId::ADMIN_LIST,
            VLBytes::new(b"meta-bytes".to_vec()),
        )
        .unwrap();
        let bytes = ComponentRegistryComponent::encode_value(&map).unwrap();
        let decoded = ComponentRegistryComponent::decode_value(&bytes).unwrap();
        assert_eq!(decoded.len(), 1);
        assert_eq!(
            decoded.get(&ComponentId::ADMIN_LIST).unwrap().as_slice(),
            b"meta-bytes"
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn component_registry_apply_update_replaces_metadata() {
        let old_meta = valid_meta_bytes();
        let mut new_meta = valid_meta_bytes();
        // Same structural validity, different type tag so the
        // replacement is observable.
        new_meta = {
            let mut decoded = xmtp_proto::xmtp::mls::message_contents::ComponentMetadata::decode(
                new_meta.as_slice(),
            )
            .unwrap();
            decoded.component_type = ComponentType::String as i32;
            decoded.encode_to_vec()
        };

        let mut prior_map: TlsMap<ComponentId, VLBytes> = TlsMap::new();
        prior_map
            .insert(ComponentId::GROUP_NAME, VLBytes::new(old_meta))
            .unwrap();
        let prior_bytes = ComponentRegistryComponent::encode_value(&prior_map).unwrap();

        let delta = TlsMapDelta::<ComponentId, VLBytes>::new()
            .update(ComponentId::GROUP_NAME, VLBytes::new(new_meta.clone()));
        let payload = ComponentRegistryComponent::encode_mutation(&delta).unwrap();
        let new_bytes =
            ComponentRegistryComponent::apply_update_payload(&payload, Some(&prior_bytes)).unwrap();
        let new = ComponentRegistryComponent::decode_value(&new_bytes).unwrap();
        assert_eq!(
            new.get(&ComponentId::GROUP_NAME).unwrap().as_slice(),
            new_meta.as_slice()
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn component_registry_apply_batched_delta_atomically() {
        // Bulk-register two custom components in one proposal.
        let delta = TlsMapDelta::<ComponentId, VLBytes>::new()
            .insert(ComponentId::new(0xC100), VLBytes::new(valid_meta_bytes()))
            .insert(ComponentId::new(0xC101), VLBytes::new(valid_meta_bytes()));
        let payload = ComponentRegistryComponent::encode_mutation(&delta).unwrap();
        let new_bytes = ComponentRegistryComponent::apply_update_payload(&payload, None).unwrap();
        let new = ComponentRegistryComponent::decode_value(&new_bytes).unwrap();
        assert_eq!(new.len(), 2);
        assert!(new.get(&ComponentId::new(0xC100)).is_some());
        assert!(new.get(&ComponentId::new(0xC101)).is_some());
    }

    // ------------------------------------------------------------------
    // Registry delta entry-validation (apply + expand must agree)
    // ------------------------------------------------------------------

    /// Apply and expand must return the same verdict for the same
    /// delta — they share `validate_registry_delta`, and this pins it.
    fn assert_registry_delta_rejected_everywhere(delta: TlsMapDelta<ComponentId, VLBytes>) {
        let payload = ComponentRegistryComponent::encode_mutation(&delta).unwrap();
        let apply_err = ComponentRegistryComponent::apply_update_payload(&payload, None);
        assert!(
            matches!(apply_err, Err(ComponentTypedError::RegistryMutation(_))),
            "apply must reject, got {apply_err:?}"
        );
        let op = AppDataUpdateOperation::Update(payload.into());
        let expand_err = ComponentRegistryComponent::expand_to_changes(&op, None);
        assert!(
            matches!(expand_err, Err(ComponentTypedError::RegistryMutation(_))),
            "expand must reject, got {expand_err:?}"
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn registry_delta_rejects_reserved_range_insert() {
        // THE poison case: a reserved-range entry accepted into the
        // dict would fail every subsequent registry load. It must be
        // rejected at the wire, on both the apply and validate paths.
        assert_registry_delta_rejected_everywhere(
            TlsMapDelta::<ComponentId, VLBytes>::new()
                .insert(ComponentId::new(0xFF00), VLBytes::new(valid_meta_bytes())),
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn registry_delta_rejects_hardcoded_insert() {
        assert_registry_delta_rejected_everywhere(
            TlsMapDelta::<ComponentId, VLBytes>::new().insert(
                ComponentId::SUPER_ADMIN_LIST,
                VLBytes::new(valid_meta_bytes()),
            ),
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn registry_delta_rejects_out_of_space_insert() {
        assert_registry_delta_rejected_everywhere(
            TlsMapDelta::<ComponentId, VLBytes>::new()
                .insert(ComponentId::new(0x0001), VLBytes::new(valid_meta_bytes())),
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn registry_delta_rejects_undecodable_metadata_insert() {
        assert_registry_delta_rejected_everywhere(
            TlsMapDelta::<ComponentId, VLBytes>::new().insert(
                ComponentId::new(0xC100),
                VLBytes::new(vec![0xFF, 0xFF, 0xFF, 0xFF]),
            ),
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn registry_delta_rejects_immutable_update_and_delete() {
        // Write-once: immutable-range registry entries can be inserted
        // but never overwritten or deleted, mirroring
        // `ComponentRegistry::{set, remove}`.
        assert_registry_delta_rejected_everywhere(
            TlsMapDelta::<ComponentId, VLBytes>::new()
                .update(ComponentId::new(0xBE00), VLBytes::new(valid_meta_bytes())),
        );
        assert_registry_delta_rejected_everywhere(
            TlsMapDelta::<ComponentId, VLBytes>::new().delete(ComponentId::new(0xBE00)),
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn registry_delta_rejects_hardcoded_delete() {
        assert_registry_delta_rejected_everywhere(
            TlsMapDelta::<ComponentId, VLBytes>::new().delete(ComponentId::SUPER_ADMIN_LIST),
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn registry_component_rejects_whole_component_remove() {
        // Deleting the entire COMPONENT_REGISTRY dict slot would strand
        // the group (the registry's presence is the migration marker
        // and the policy source). Never legal.
        let err =
            ComponentRegistryComponent::expand_to_changes(&AppDataUpdateOperation::Remove, None);
        assert!(
            matches!(err, Err(ComponentTypedError::RegistryMutation(_))),
            "Remove of the registry component must be rejected, got {err:?}"
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn registry_delta_valid_mutations_expand_and_apply() {
        // The happy path still works end-to-end after entry validation:
        // register a custom component, then observe it in both the
        // expanded changes and the applied map.
        let delta = TlsMapDelta::<ComponentId, VLBytes>::new()
            .insert(ComponentId::new(0xC200), VLBytes::new(valid_meta_bytes()));
        let payload = ComponentRegistryComponent::encode_mutation(&delta).unwrap();

        let op = AppDataUpdateOperation::Update(payload.clone().into());
        let changes = ComponentRegistryComponent::expand_to_changes(&op, None).unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].op, ComponentOp::Insert);

        let new_bytes = ComponentRegistryComponent::apply_update_payload(&payload, None).unwrap();
        let new = ComponentRegistryComponent::decode_value(&new_bytes).unwrap();
        assert!(new.get(&ComponentId::new(0xC200)).is_some());
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn apply_rejects_non_delta_payload() {
        // The wire is always a `TlsMapDelta`, never a raw `TlsMap`.
        // A caller that mistakenly emitted a snapshot as the payload
        // must surface as a decode failure, not silently overwrite
        // the dict.
        let mut map: TlsMap<InboxId, VLBytes> = TlsMap::new();
        map.insert(fixture_inbox_id(1), VLBytes::new(b"v1".to_vec()))
            .unwrap();
        let raw_map_bytes = map.tls_serialize_detached().unwrap();
        let err = GroupMembershipComponent::apply_update_payload(&raw_map_bytes, None).unwrap_err();
        assert!(
            matches!(err, ComponentTypedError::TlsCodec(_)),
            "expected TlsCodec decode error for non-delta payload, got {err:?}"
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn component_types_are_correct() {
        assert_eq!(
            GroupMembershipComponent::COMPONENT_TYPE,
            ComponentType::TlsMapInboxIdBytes
        );
        assert_eq!(
            ComponentRegistryComponent::COMPONENT_TYPE,
            ComponentType::TlsMapBytesBytes
        );
    }
}
