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
        component_registry::ComponentOp,
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
/// `ComponentId`-keyed maps share the same body.
fn apply_tls_map_delta<K>(
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
fn expand_tls_map_changes<K>(
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
    }
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
pub struct ComponentRegistryComponent;

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
        apply_tls_map_delta::<ComponentId>(payload, prior)
    }

    fn expand_to_changes(
        op: &AppDataUpdateOperation,
        prior: Option<&[u8]>,
    ) -> Result<Vec<ExpandedComponentChange>, ComponentTypedError> {
        expand_tls_map_changes::<ComponentId>(op, prior)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_inbox_id(seed: u8) -> InboxId {
        let mut bytes = [0u8; 32];
        bytes[0] = seed;
        InboxId::from_bytes(bytes)
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
        map.insert(ComponentId::ADMIN_LIST, VLBytes::new(b"meta-bytes".to_vec()))
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
        let mut prior_map: TlsMap<ComponentId, VLBytes> = TlsMap::new();
        prior_map
            .insert(ComponentId::GROUP_NAME, VLBytes::new(b"old-policy".to_vec()))
            .unwrap();
        let prior_bytes = ComponentRegistryComponent::encode_value(&prior_map).unwrap();

        let delta = TlsMapDelta::<ComponentId, VLBytes>::new()
            .update(ComponentId::GROUP_NAME, VLBytes::new(b"new-policy".to_vec()));
        let payload = ComponentRegistryComponent::encode_mutation(&delta).unwrap();
        let new_bytes =
            ComponentRegistryComponent::apply_update_payload(&payload, Some(&prior_bytes)).unwrap();
        let new = ComponentRegistryComponent::decode_value(&new_bytes).unwrap();
        assert_eq!(
            new.get(&ComponentId::GROUP_NAME).unwrap().as_slice(),
            b"new-policy"
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn component_registry_apply_batched_delta_atomically() {
        // Bulk-register two custom components in one proposal.
        let delta = TlsMapDelta::<ComponentId, VLBytes>::new()
            .insert(ComponentId::new(0xC100), VLBytes::new(b"meta1".to_vec()))
            .insert(ComponentId::new(0xC101), VLBytes::new(b"meta2".to_vec()));
        let payload = ComponentRegistryComponent::encode_mutation(&delta).unwrap();
        let new_bytes = ComponentRegistryComponent::apply_update_payload(&payload, None).unwrap();
        let new = ComponentRegistryComponent::decode_value(&new_bytes).unwrap();
        assert_eq!(new.len(), 2);
        assert!(new.get(&ComponentId::new(0xC100)).is_some());
        assert!(new.get(&ComponentId::new(0xC101)).is_some());
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
        let err =
            GroupMembershipComponent::apply_update_payload(&raw_map_bytes, None).unwrap_err();
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
