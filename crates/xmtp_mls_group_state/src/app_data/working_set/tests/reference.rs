//! The byte-level apply as it stood before [`WorkingSet`](super::WorkingSet),
//! frozen at `ab6fdcd20f` as the reference the differential tests compare
//! against. Do not change it to follow production code: it pins the
//! verdicts every older client reaches.
//!
//! Frozen here, because the working set implements them again:
//! `apply_app_data_update_payload` with the bounds and group-root checks it
//! runs, and the apply of every built-in and type-dispatched collection,
//! with `TlsMap::apply_delta` and `TlsSet::apply_delta` as they stood
//! before they became linear. Shared with production, because the working
//! set calls them instead of implementing them: the tls_codec encodings of
//! the collections and their deltas, the scalar codecs
//! (`Component::apply_update_payload` of a scalar, and
//! `apply_update_payload_for_type` for `Bytes`, `String` and
//! `Unspecified`), the registry's write invariants
//! (`validate_registry_delta`) and entry decoding, and the well-known type
//! and impl tables.

use std::collections::{BTreeMap, HashMap, btree_map};

use openmls::messages::proposals::AppDataUpdateOperation;
use tls_codec::{Deserialize, Serialize, Size, VLBytes};
use xmtp_cryptography::secret::Zeroizing;
use xmtp_mls_group_keys::group_keys::GROUP_ROOT_BYTES;
use xmtp_mls_tls_collections::{
    tls_map::{TlsMap, TlsMapDelta, TlsMapError, TlsMapMutation},
    tls_set::{TlsKeyHash, TlsSet, TlsSetDelta, TlsSetError, TlsSetMutation},
};
use xmtp_proto::xmtp::mls::message_contents::ComponentType;

use crate::{
    app_data::{
        component_id::ComponentId,
        component_registry::ComponentRegistry,
        component_source::{
            ComponentSourceError, MAX_FIELD_ELEMENT_BYTES, MAX_FIELD_SNAPSHOT_BYTES, component_type,
        },
        components::{
            metadata_attributes::decode_utf8, tls_map_components::validate_registry_delta,
            type_dispatch::apply_update_payload_for_type,
        },
        registry_table::lookup_component,
        typed::{ComponentTypedError, ErasedComponent},
        working_set::counters,
    },
    inbox_id::InboxId,
};

/// Each touched component's state after a proposal sequence: its bytes, or
/// `None` once removed.
pub(super) type States = BTreeMap<ComponentId, Option<Zeroizing<Vec<u8>>>>;

/// A byte-level apply of one `Update` payload: this module's frozen
/// [`apply_app_data_update_payload`], or production's.
pub(super) type Apply = fn(
    ComponentId,
    &[u8],
    Option<&[u8]>,
    &ComponentRegistry,
) -> Result<Zeroizing<Vec<u8>>, ComponentSourceError>;

/// Apply `operations` in order over `base`, as `AppDataBatch` does. Fails
/// with the index and error of the first update that fails.
pub(super) fn apply_sequence(
    registry: &ComponentRegistry,
    base: &BTreeMap<ComponentId, Vec<u8>>,
    operations: &[(ComponentId, AppDataUpdateOperation)],
) -> Result<States, (usize, ComponentSourceError)> {
    let mut states = States::new();
    for (index, (id, operation)) in operations.iter().enumerate() {
        apply_one(registry, base, &mut states, *id, operation).map_err(|error| (index, error))?;
    }
    Ok(states)
}

/// Apply one operation to `states` over `base`: an `Update` against the
/// state the earlier operations left, a `Remove` to no value.
pub(super) fn apply_one(
    registry: &ComponentRegistry,
    base: &BTreeMap<ComponentId, Vec<u8>>,
    states: &mut States,
    id: ComponentId,
    operation: &AppDataUpdateOperation,
) -> Result<(), ComponentSourceError> {
    apply_one_with(
        apply_app_data_update_payload,
        registry,
        base,
        states,
        id,
        operation,
    )
}

/// [`apply_one`] with the `Update` payload applied by `apply`.
pub(super) fn apply_one_with(
    apply: Apply,
    registry: &ComponentRegistry,
    base: &BTreeMap<ComponentId, Vec<u8>>,
    states: &mut States,
    id: ComponentId,
    operation: &AppDataUpdateOperation,
) -> Result<(), ComponentSourceError> {
    let state = match operation {
        AppDataUpdateOperation::Update(payload) => Some(apply(
            id,
            payload.as_slice(),
            current(states, base, id),
            registry,
        )?),
        AppDataUpdateOperation::Remove => None,
    };
    states.insert(id, state);
    Ok(())
}

/// Component `id`'s bytes after the operations in `states`.
pub(super) fn current<'s>(
    states: &'s States,
    base: &'s BTreeMap<ComponentId, Vec<u8>>,
    id: ComponentId,
) -> Option<&'s [u8]> {
    match states.get(&id) {
        Some(state) => state.as_deref().map(Vec::as_slice),
        None => base.get(&id).map(Vec::as_slice),
    }
}

// --- Frozen copy of `component_source.rs` at `ab6fdcd20f`. ---

pub(super) fn apply_app_data_update_payload(
    id: ComponentId,
    payload: &[u8],
    old_value: Option<&[u8]>,
    registry: &ComponentRegistry,
) -> Result<Zeroizing<Vec<u8>>, ComponentSourceError> {
    check_update_payload_bounds(id, payload, registry)?;
    if id.is_immutable() && old_value.is_some() {
        return Err(ComponentSourceError::ImmutableUpdate(id));
    }
    let new_value = Zeroizing::new(match lookup_component(id) {
        Some(component) => apply_built_in(id, component, payload, old_value)?,
        None => apply_for_type(id, resolved_type(id, registry)?, payload, old_value)?,
    });
    if is_field_bounded(id) {
        check_field_bounds(id, resolved_type(id, registry)?, &new_value)?;
    }
    check_group_root_length(id, &new_value)?;
    Ok(new_value)
}

fn check_group_root_length(id: ComponentId, value: &[u8]) -> Result<(), ComponentSourceError> {
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

// --- Frozen copy of `component_source/bounds.rs` at `ab6fdcd20f`. ---

pub(super) fn resolved_type(
    id: ComponentId,
    registry: &ComponentRegistry,
) -> Result<ComponentType, ComponentSourceError> {
    component_type(id).map_or_else(|| registered_component_type(id, registry), Ok)
}

fn is_field_bounded(id: ComponentId) -> bool {
    id.is_app_range()
        || [
            ComponentId::USER_DISPLAY_NAME,
            ComponentId::GROUP_IMAGE,
            ComponentId::COMPONENT_REGISTRY,
        ]
        .contains(&id)
}

fn check_update_payload_bounds(
    id: ComponentId,
    payload: &[u8],
    registry: &ComponentRegistry,
) -> Result<(), ComponentSourceError> {
    if !is_field_bounded(id) && id != ComponentId::GROUP_MEMBERSHIP {
        return Ok(());
    }
    let longest = match resolved_type(id, registry)? {
        ComponentType::Bytes | ComponentType::String => Some(payload.len()),
        ComponentType::TlsSetBytes => TlsSetDelta::<VLBytes>::tls_deserialize_exact(payload)?
            .mutations
            .iter()
            .map(|m| match m {
                TlsSetMutation::Insert(key) | TlsSetMutation::Remove(key) => key.as_slice().len(),
                TlsSetMutation::RemoveByHash(_) => 0,
            })
            .max(),
        ComponentType::TlsMapInboxIdBytes | ComponentType::TlsMapInboxIdString => {
            longest_delta_value::<InboxId>(payload)?
        }
        ComponentType::TlsMapBytesBytes if id == ComponentId::COMPONENT_REGISTRY => {
            longest_delta_value::<ComponentId>(payload)?
        }
        ComponentType::TlsMapBytesBytes => {
            TlsMapDelta::<VLBytes, VLBytes>::tls_deserialize_exact(payload)?
                .mutations
                .iter()
                .map(|m| match m {
                    TlsMapMutation::Insert { key, value }
                    | TlsMapMutation::Update { key, value } => {
                        key.as_slice().len().max(value.as_slice().len())
                    }
                    TlsMapMutation::Delete { key } => key.as_slice().len(),
                })
                .max()
        }
        ComponentType::TlsSetInboxId | ComponentType::Unspecified => None,
    };
    check_element_bound(id, longest.unwrap_or(0))
}

fn longest_delta_value<K>(payload: &[u8]) -> Result<Option<usize>, tls_codec::Error>
where
    TlsMapDelta<K, VLBytes>: Deserialize,
{
    Ok(TlsMapDelta::<K, VLBytes>::tls_deserialize_exact(payload)?
        .mutations
        .iter()
        .map(|m| match m {
            TlsMapMutation::Insert { value, .. } | TlsMapMutation::Update { value, .. } => {
                value.as_slice().len()
            }
            TlsMapMutation::Delete { .. } => 0,
        })
        .max())
}

fn check_field_bounds(
    id: ComponentId,
    ty: ComponentType,
    value: &[u8],
) -> Result<(), ComponentSourceError> {
    let longest_element = match ty {
        ComponentType::Bytes | ComponentType::String => Some(value.len()),
        _ if value.len() > MAX_FIELD_SNAPSHOT_BYTES => {
            return Err(ComponentSourceError::FieldBoundExceeded {
                component_id: id,
                len: value.len(),
                max: MAX_FIELD_SNAPSHOT_BYTES,
            });
        }
        ComponentType::TlsSetBytes => TlsSet::<VLBytes>::tls_deserialize_exact(value)?
            .iter()
            .map(|v| v.as_slice().len())
            .max(),
        ComponentType::TlsMapInboxIdBytes | ComponentType::TlsMapInboxIdString => {
            longest_map_value::<InboxId>(value)?
        }
        ComponentType::TlsMapBytesBytes if id == ComponentId::COMPONENT_REGISTRY => {
            longest_map_value::<ComponentId>(value)?
        }
        ComponentType::TlsMapBytesBytes => {
            TlsMap::<VLBytes, VLBytes>::tls_deserialize_exact(value)?
                .iter()
                .flat_map(|(k, v)| [k.as_slice().len(), v.as_slice().len()])
                .max()
        }
        ComponentType::TlsSetInboxId | ComponentType::Unspecified => None,
    }
    .unwrap_or(0);
    check_element_bound(id, longest_element)
}

fn longest_map_value<K: Ord>(value: &[u8]) -> Result<Option<usize>, tls_codec::Error>
where
    TlsMap<K, VLBytes>: Deserialize,
{
    Ok(TlsMap::<K, VLBytes>::tls_deserialize_exact(value)?
        .values()
        .map(|v| v.as_slice().len())
        .max())
}

fn check_element_bound(id: ComponentId, len: usize) -> Result<(), ComponentSourceError> {
    if len > MAX_FIELD_ELEMENT_BYTES {
        return Err(ComponentSourceError::FieldBoundExceeded {
            component_id: id,
            len,
            max: MAX_FIELD_ELEMENT_BYTES,
        });
    }
    Ok(())
}

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

// --- Frozen copy of the collection apply at `ab6fdcd20f`: the built-in
// impls in `components/`, `apply_update_payload_for_type`, and
// `TlsMap::apply_delta` and `TlsSet::apply_delta`. A collection decodes
// into a `BTreeMap`, which orders keys as `TlsMap` does, and a failed delta
// drops it, so the apply needs no undo. ---

/// A collection key the frozen apply decodes and encodes.
trait Key: Ord + Clone + Serialize + Deserialize + Size + std::fmt::Debug {}

impl<K: Ord + Clone + Serialize + Deserialize + Size + std::fmt::Debug> Key for K {}

/// A built-in impl's `apply_update_payload`. Scalars keep the shared codec.
fn apply_built_in(
    id: ComponentId,
    component: &dyn ErasedComponent,
    payload: &[u8],
    prior: Option<&[u8]>,
) -> Result<Vec<u8>, ComponentTypedError> {
    match component.component_type() {
        // `AdminListComponent`, `SuperAdminListComponent` and
        // `DmMembersComponent` call `apply_inbox_id_set_delta`.
        ComponentType::TlsSetInboxId => apply_set::<InboxId>(payload, prior),
        // `GroupMembershipComponent` calls `apply_tls_map_delta::<InboxId>`.
        ComponentType::TlsMapInboxIdBytes => apply_map::<InboxId>(payload, prior),
        // `ComponentRegistryComponent`.
        ComponentType::TlsMapBytesBytes if id == ComponentId::COMPONENT_REGISTRY => {
            let delta = TlsMapDelta::<ComponentId, VLBytes>::tls_deserialize_exact(payload)?;
            validate_registry_delta(&delta)?;
            apply_map_delta(delta, prior)
        }
        _ => component.apply_update_payload(payload, prior),
    }
}

/// `apply_update_payload_for_type`. Scalars keep the shared codec.
fn apply_for_type(
    id: ComponentId,
    ty: ComponentType,
    payload: &[u8],
    prior: Option<&[u8]>,
) -> Result<Vec<u8>, ComponentTypedError> {
    let result = match ty {
        ComponentType::Bytes | ComponentType::String | ComponentType::Unspecified => {
            return apply_update_payload_for_type(id, ty, payload, prior);
        }
        ComponentType::TlsSetInboxId => apply_set::<InboxId>(payload, prior),
        ComponentType::TlsSetBytes => apply_set::<VLBytes>(payload, prior),
        ComponentType::TlsMapInboxIdBytes => apply_map::<InboxId>(payload, prior),
        ComponentType::TlsMapInboxIdString => {
            // `?` returns a codec error before the id is attached below.
            let snapshot = apply_map::<InboxId>(payload, prior)?;
            let map = TlsMap::<InboxId, VLBytes>::tls_deserialize_exact(&snapshot)?;
            for value in map.values() {
                decode_utf8(id, value.as_slice())?;
            }
            Ok(snapshot)
        }
        ComponentType::TlsMapBytesBytes => apply_map::<VLBytes>(payload, prior),
    };
    // `attach_component_id`.
    result.map_err(|error| match error {
        ComponentTypedError::TlsCodec(inner) => ComponentTypedError::MalformedValue {
            component_id: id,
            reason: format!("type-aware apply ({ty:?}) tls codec error: {inner}"),
        },
        other => other,
    })
}

/// `apply_inbox_id_set_delta` and `apply_bytes_set_delta`.
fn apply_set<K: Key>(payload: &[u8], prior: Option<&[u8]>) -> Result<Vec<u8>, ComponentTypedError> {
    let delta = TlsSetDelta::<K>::tls_deserialize_exact(payload)?;
    let mut set: BTreeMap<K, ()> = match prior {
        Some(bytes) => TlsSet::<K>::tls_deserialize_exact(bytes)?
            .into_iter()
            .map(|key| (key, ()))
            .collect(),
        None => BTreeMap::new(),
    };
    set_apply_delta(&mut set, delta.mutations)?;
    Ok(TlsSet::from_keys(set.into_keys()).tls_serialize_detached()?)
}

/// `apply_tls_map_delta`.
fn apply_map<K: Key>(payload: &[u8], prior: Option<&[u8]>) -> Result<Vec<u8>, ComponentTypedError> {
    apply_map_delta(
        TlsMapDelta::<K, VLBytes>::tls_deserialize_exact(payload)?,
        prior,
    )
}

/// The rest of `apply_tls_map_delta`, once the delta is decoded.
fn apply_map_delta<K: Key>(
    delta: TlsMapDelta<K, VLBytes>,
    prior: Option<&[u8]>,
) -> Result<Vec<u8>, ComponentTypedError> {
    let mut map: BTreeMap<K, VLBytes> = match prior {
        Some(bytes) => TlsMap::<K, VLBytes>::tls_deserialize_exact(bytes)?
            .into_iter()
            .collect(),
        None => BTreeMap::new(),
    };
    map_apply_delta(&mut map, delta.mutations)?;
    Ok(TlsMap::from_pairs(map).tls_serialize_detached()?)
}

/// `TlsMap::apply_delta`: each mutation in order, to the map the ones before
/// it left, and the first that fails decides the error.
fn map_apply_delta<K: Ord, V>(
    map: &mut BTreeMap<K, V>,
    mutations: Vec<TlsMapMutation<K, V>>,
) -> Result<(), TlsMapError> {
    for mutation in mutations {
        match mutation {
            TlsMapMutation::Insert { key, value } => match map.entry(key) {
                btree_map::Entry::Occupied(_) => return Err(TlsMapError::KeyExists),
                btree_map::Entry::Vacant(slot) => {
                    slot.insert(value);
                }
            },
            TlsMapMutation::Update { key, value } => {
                *map.get_mut(&key).ok_or(TlsMapError::KeyNotFound)? = value;
            }
            TlsMapMutation::Delete { key } => {
                map.remove(&key).ok_or(TlsMapError::KeyNotFound)?;
            }
        }
    }
    Ok(())
}

/// `TlsSet::apply_delta`: every `RemoveByHash` resolves against the set
/// before the delta, through an index of every key's hash that fails on a
/// clash, before any mutation applies.
fn set_apply_delta<K: Key>(
    set: &mut BTreeMap<K, ()>,
    mutations: Vec<TlsSetMutation<K>>,
) -> Result<(), TlsSetError> {
    let resolved = {
        let has_hash_removal = mutations
            .iter()
            .any(|m| matches!(m, TlsSetMutation::RemoveByHash(_)));
        let hash_index: Option<HashMap<TlsKeyHash, &K>> = if has_hash_removal {
            let mut index = HashMap::with_capacity(set.len());
            for key in set.keys() {
                if index.insert(key_hash(key)?, key).is_some() {
                    return Err(TlsSetError::DuplicateHash);
                }
            }
            Some(index)
        } else {
            None
        };
        mutations
            .into_iter()
            .map(|m| -> Result<TlsMapMutation<K, ()>, TlsSetError> {
                match m {
                    TlsSetMutation::Insert(key) => Ok(TlsMapMutation::Insert { key, value: () }),
                    TlsSetMutation::Remove(key) => Ok(TlsMapMutation::Delete { key }),
                    TlsSetMutation::RemoveByHash(target_hash) => {
                        let index = hash_index
                            .as_ref()
                            .expect("hash index must be built for RemoveByHash");
                        let key = index.get(&target_hash).ok_or(TlsMapError::KeyNotFound)?;
                        Ok(TlsMapMutation::Delete {
                            key: (*key).clone(),
                        })
                    }
                }
            })
            .collect::<Result<Vec<_>, _>>()?
    };
    map_apply_delta(set, resolved)?;
    Ok(())
}

/// `TlsKeyHash::of`, or inside `counters::with_hash_clash` the hash every
/// key shares.
fn key_hash<K: Serialize + Size>(key: &K) -> Result<TlsKeyHash, tls_codec::Error> {
    Ok(counters::key_hash(TlsKeyHash::of(key)?))
}
