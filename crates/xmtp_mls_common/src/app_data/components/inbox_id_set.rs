//! [`Component`] impls for the `TlsSet<InboxId>`-shaped components:
//! [`AdminListComponent`] (`ADMIN_LIST`),
//! [`SuperAdminListComponent`] (`SUPER_ADMIN_LIST`), and
//! [`DmMembersComponent`] (`DM_MEMBERS`, immutable).
//!
//! All three share an identical wire format and trait shape. They
//! differ only in their `ComponentId` and the dispatch layer's
//! immutability gate (`DM_MEMBERS` is in the immutable range, so the
//! dispatch layer rejects `Update` writes before they reach this impl
//! — but the impl is still valid as a `read` path).

use openmls::messages::proposals::AppDataUpdateOperation;
use std::collections::HashMap;
use tls_codec::{Deserialize, Serialize};
use xmtp_proto::xmtp::mls::message_contents::ComponentType;

use crate::{
    app_data::{
        component_id::ComponentId,
        component_registry::ComponentOp,
        typed::{Component, ComponentTypedError, ExpandedComponentChange},
    },
    inbox_id::InboxId,
    tls_set::{TlsKeyHash, TlsSet, TlsSetDelta, TlsSetError, TlsSetMutation},
};

/// Apply a `TlsSetDelta<InboxId>` wire payload over the prior dict
/// bytes (a `TlsSet<InboxId>` snapshot, or `None` if this is the first
/// write — bootstrap, where prior state is empty).
///
/// Returns the new dict bytes: the re-serialized `TlsSet<InboxId>`
/// snapshot. The dict always holds the raw set as state; the wire
/// always carries a delta describing the change. This function is
/// the one boundary that translates between them.
///
/// Shared between the three inbox-id-set impls.
fn apply_inbox_id_set_delta(
    payload: &[u8],
    prior: Option<&[u8]>,
) -> Result<Vec<u8>, ComponentTypedError> {
    let delta = TlsSetDelta::<InboxId>::tls_deserialize_exact(payload)?;
    let mut set: TlsSet<InboxId> = match prior {
        Some(bytes) => TlsSet::<InboxId>::tls_deserialize_exact(bytes)?,
        None => TlsSet::new(),
    };
    set.apply_delta(delta)?;
    Ok(set.tls_serialize_detached()?)
}

/// Expand an `AppDataUpdate` proposal for an inbox-id-set component
/// into the per-element `ExpandedComponentChange` entries the validator
/// iterates over.
///
/// Mirrors the existing `expand_app_data_update_to_changes`
/// implementation in `xmtp_mls::groups::app_data::component_source`
/// (which #7 will retire). `RemoveByHash` mutations resolve back to
/// the underlying `InboxId` via a hash index built from the prior set.
fn expand_inbox_id_set_changes(
    op: &AppDataUpdateOperation,
    prior: Option<&[u8]>,
) -> Result<Vec<ExpandedComponentChange>, ComponentTypedError> {
    match op {
        AppDataUpdateOperation::Remove => Ok(vec![ExpandedComponentChange {
            op: ComponentOp::Delete,
            value: None,
        }]),
        AppDataUpdateOperation::Update(payload) => {
            let delta = TlsSetDelta::<InboxId>::tls_deserialize_exact(payload.as_slice())?;

            // Lazily build a hash → InboxId index only if we actually
            // see a `RemoveByHash` in this delta. Common case
            // (Insert/Remove only) skips the prior-set decode.
            let needs_index = delta
                .mutations
                .iter()
                .any(|m| matches!(m, TlsSetMutation::RemoveByHash(_)));
            let hash_index: Option<HashMap<TlsKeyHash, InboxId>> = if needs_index {
                match prior {
                    Some(bytes) => {
                        let prior_set = TlsSet::<InboxId>::tls_deserialize_exact(bytes)?;
                        let mut idx = HashMap::with_capacity(prior_set.len());
                        for key in prior_set.iter() {
                            // A hash clash between two distinct keys
                            // is cryptographically infeasible with
                            // SHA-256, but defense-in-depth: reject
                            // any silently-lossy index. Matches
                            // `TlsSet::apply_delta`'s `DuplicateHash`
                            // check at apply time.
                            if idx.insert(TlsKeyHash::of(key)?, *key).is_some() {
                                return Err(ComponentTypedError::TlsSetApply(
                                    TlsSetError::DuplicateHash,
                                ));
                            }
                        }
                        Some(idx)
                    }
                    // No prior bytes → empty set → every RemoveByHash
                    // trivially misses. Skip the allocation; each
                    // lookup returns None.
                    None => None,
                }
            } else {
                None
            };

            let mut out = Vec::with_capacity(delta.mutations.len());
            for mutation in delta.mutations {
                match mutation {
                    TlsSetMutation::Insert(key) => out.push(ExpandedComponentChange {
                        op: ComponentOp::Insert,
                        value: Some(key.into_bytes().to_vec()),
                    }),
                    TlsSetMutation::Remove(key) => out.push(ExpandedComponentChange {
                        op: ComponentOp::Delete,
                        value: Some(key.into_bytes().to_vec()),
                    }),
                    TlsSetMutation::RemoveByHash(target) => {
                        let resolved = hash_index
                            .as_ref()
                            .and_then(|idx| idx.get(&target))
                            .map(|id| id.as_bytes().to_vec());
                        out.push(ExpandedComponentChange {
                            op: ComponentOp::Delete,
                            value: resolved,
                        });
                    }
                }
            }
            Ok(out)
        }
    }
}

macro_rules! inbox_id_set_component {
    ($struct_name:ident, $id:expr) => {
        pub struct $struct_name;

        impl Component for $struct_name {
            const ID: ComponentId = $id;
            const COMPONENT_TYPE: ComponentType = ComponentType::TlsSetInboxId;
            type Value = TlsSet<InboxId>;
            // The mutation type is the full wire-level delta so a
            // single proposal can carry multiple Insert/Remove
            // mutations atomically. Single-mutation callers build a
            // one-element delta via `TlsSetDelta::new().insert(x)`.
            type Mutation = TlsSetDelta<InboxId>;

            fn decode_value(bytes: &[u8]) -> Result<Self::Value, ComponentTypedError> {
                TlsSet::<InboxId>::tls_deserialize_exact(bytes).map_err(Into::into)
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
                apply_inbox_id_set_delta(payload, prior)
            }

            fn expand_to_changes(
                op: &AppDataUpdateOperation,
                prior: Option<&[u8]>,
            ) -> Result<Vec<ExpandedComponentChange>, ComponentTypedError> {
                expand_inbox_id_set_changes(op, prior)
            }
        }
    };
}

inbox_id_set_component!(AdminListComponent, ComponentId::ADMIN_LIST);
inbox_id_set_component!(SuperAdminListComponent, ComponentId::SUPER_ADMIN_LIST);
inbox_id_set_component!(DmMembersComponent, ComponentId::DM_MEMBERS);

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_inbox_id(seed: u8) -> InboxId {
        let mut bytes = [0u8; 32];
        bytes[0] = seed;
        InboxId::from_bytes(bytes)
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn round_trip_admin_list_value() {
        let mut set = TlsSet::<InboxId>::new();
        set.insert(fixture_inbox_id(1)).unwrap();
        set.insert(fixture_inbox_id(2)).unwrap();
        let bytes = AdminListComponent::encode_value(&set).unwrap();
        let decoded = AdminListComponent::decode_value(&bytes).unwrap();
        assert_eq!(decoded.len(), 2);
        assert!(decoded.contains(&fixture_inbox_id(1)));
        assert!(decoded.contains(&fixture_inbox_id(2)));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn encode_mutation_serializes_full_delta() {
        let delta = TlsSetDelta::<InboxId>::new().insert(fixture_inbox_id(7));
        let bytes = AdminListComponent::encode_mutation(&delta).unwrap();
        let round_trip = TlsSetDelta::<InboxId>::tls_deserialize_exact(&bytes).unwrap();
        assert_eq!(round_trip.mutations.len(), 1);
        match &round_trip.mutations[0] {
            TlsSetMutation::Insert(id) => assert_eq!(*id, fixture_inbox_id(7)),
            other => panic!("unexpected mutation: {other:?}"),
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn encode_mutation_supports_batched_delta() {
        // The motivating case for delta-as-mutation: multiple
        // changes in one proposal. e.g. promote two new admins and
        // demote a third atomically.
        let delta = TlsSetDelta::<InboxId>::new()
            .insert(fixture_inbox_id(1))
            .insert(fixture_inbox_id(2))
            .remove(fixture_inbox_id(3));
        let bytes = AdminListComponent::encode_mutation(&delta).unwrap();
        let round_trip = TlsSetDelta::<InboxId>::tls_deserialize_exact(&bytes).unwrap();
        assert_eq!(round_trip.mutations.len(), 3);
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn apply_insert_against_empty_prior() {
        let delta = TlsSetDelta::<InboxId>::new().insert(fixture_inbox_id(3));
        let payload = AdminListComponent::encode_mutation(&delta).unwrap();
        let new_bytes = AdminListComponent::apply_update_payload(&payload, None).unwrap();
        let new = AdminListComponent::decode_value(&new_bytes).unwrap();
        assert_eq!(new.len(), 1);
        assert!(new.contains(&fixture_inbox_id(3)));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn apply_remove_against_existing_prior() {
        // Build a prior with two members.
        let mut prior_set = TlsSet::<InboxId>::new();
        prior_set.insert(fixture_inbox_id(1)).unwrap();
        prior_set.insert(fixture_inbox_id(2)).unwrap();
        let prior_bytes = SuperAdminListComponent::encode_value(&prior_set).unwrap();

        // Send a Remove(inbox_id_1) delta.
        let delta = TlsSetDelta::<InboxId>::new().remove(fixture_inbox_id(1));
        let payload = SuperAdminListComponent::encode_mutation(&delta).unwrap();

        let new_bytes =
            SuperAdminListComponent::apply_update_payload(&payload, Some(&prior_bytes)).unwrap();
        let new = SuperAdminListComponent::decode_value(&new_bytes).unwrap();
        assert_eq!(new.len(), 1);
        assert!(new.contains(&fixture_inbox_id(2)));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn apply_batched_delta_atomically() {
        // Apply Insert(1) + Insert(2) + Remove(3) in one shot.
        let mut prior_set = TlsSet::<InboxId>::new();
        prior_set.insert(fixture_inbox_id(3)).unwrap();
        let prior_bytes = AdminListComponent::encode_value(&prior_set).unwrap();

        let delta = TlsSetDelta::<InboxId>::new()
            .insert(fixture_inbox_id(1))
            .insert(fixture_inbox_id(2))
            .remove(fixture_inbox_id(3));
        let payload = AdminListComponent::encode_mutation(&delta).unwrap();

        let new_bytes =
            AdminListComponent::apply_update_payload(&payload, Some(&prior_bytes)).unwrap();
        let new = AdminListComponent::decode_value(&new_bytes).unwrap();
        assert_eq!(new.len(), 2);
        assert!(new.contains(&fixture_inbox_id(1)));
        assert!(new.contains(&fixture_inbox_id(2)));
        assert!(!new.contains(&fixture_inbox_id(3)));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn expand_insert_yields_single_change() {
        let delta = TlsSetDelta::<InboxId>::new().insert(fixture_inbox_id(5));
        let payload = AdminListComponent::encode_mutation(&delta).unwrap();
        let op = AppDataUpdateOperation::Update(payload.into());
        let changes = AdminListComponent::expand_to_changes(&op, None).unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].op, ComponentOp::Insert);
        assert_eq!(
            changes[0].value.as_deref(),
            Some(&fixture_inbox_id(5).as_bytes()[..])
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn expand_batched_delta_yields_one_change_per_mutation() {
        // Pinned: per-element validation iterates per-mutation, so a
        // batched delta produces one ExpandedComponentChange per
        // entry.
        let delta = TlsSetDelta::<InboxId>::new()
            .insert(fixture_inbox_id(10))
            .insert(fixture_inbox_id(11))
            .remove(fixture_inbox_id(12));
        let payload = AdminListComponent::encode_mutation(&delta).unwrap();
        let op = AppDataUpdateOperation::Update(payload.into());
        let changes = AdminListComponent::expand_to_changes(&op, None).unwrap();
        assert_eq!(changes.len(), 3);
        assert_eq!(changes[0].op, ComponentOp::Insert);
        assert_eq!(changes[1].op, ComponentOp::Insert);
        assert_eq!(changes[2].op, ComponentOp::Delete);
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn expand_remove_by_hash_resolves_against_prior() {
        let target = fixture_inbox_id(9);
        let mut prior_set = TlsSet::<InboxId>::new();
        prior_set.insert(target).unwrap();
        let prior_bytes = AdminListComponent::encode_value(&prior_set).unwrap();

        let target_hash = TlsKeyHash::of(&target).unwrap();
        let delta = TlsSetDelta::<InboxId>::new().remove_by_hash(target_hash);
        let payload = AdminListComponent::encode_mutation(&delta).unwrap();
        let op = AppDataUpdateOperation::Update(payload.into());

        let changes =
            AdminListComponent::expand_to_changes(&op, Some(&prior_bytes)).unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].op, ComponentOp::Delete);
        assert_eq!(changes[0].value.as_deref(), Some(&target.as_bytes()[..]));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn expand_remove_by_hash_with_no_prior_yields_unresolved() {
        let target_hash = TlsKeyHash::of(&fixture_inbox_id(9)).unwrap();
        let delta = TlsSetDelta::<InboxId>::new().remove_by_hash(target_hash);
        let payload = AdminListComponent::encode_mutation(&delta).unwrap();
        let op = AppDataUpdateOperation::Update(payload.into());

        let changes = AdminListComponent::expand_to_changes(&op, None).unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].op, ComponentOp::Delete);
        assert!(changes[0].value.is_none());
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn apply_rejects_non_delta_payload() {
        // The wire is always a `TlsSetDelta<InboxId>`, never a raw
        // `TlsSet`. A bootstrap caller that mistakenly emitted a full
        // set as the payload must surface as a decode failure, not
        // silently overwrite the dict.
        let mut set = TlsSet::<InboxId>::new();
        set.insert(fixture_inbox_id(1)).unwrap();
        let raw_set_bytes = set.tls_serialize_detached().unwrap();
        let err = AdminListComponent::apply_update_payload(&raw_set_bytes, None).unwrap_err();
        assert!(
            matches!(err, ComponentTypedError::TlsCodec(_)),
            "expected TlsCodec decode error for non-delta payload, got {err:?}"
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn dm_members_uses_same_codec() {
        // Identical to admin_list — sanity check that the macro
        // expansion works for the immutable-component variant too.
        let mut set = TlsSet::<InboxId>::new();
        set.insert(fixture_inbox_id(42)).unwrap();
        let bytes = DmMembersComponent::encode_value(&set).unwrap();
        let decoded = DmMembersComponent::decode_value(&bytes).unwrap();
        assert!(decoded.contains(&fixture_inbox_id(42)));
        assert_eq!(DmMembersComponent::COMPONENT_TYPE, ComponentType::TlsSetInboxId);
    }
}
