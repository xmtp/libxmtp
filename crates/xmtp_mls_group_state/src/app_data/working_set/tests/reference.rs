//! The byte-level apply as it stood before [`WorkingSet`](super::WorkingSet),
//! frozen at `ab6fdcd20f` as the reference the differential tests compare
//! against. Do not change it to follow production code: it pins the
//! verdicts every older client reaches.
//!
//! Copied verbatim: `apply_app_data_update_payload` and the bounds and
//! group-root checks it runs. Shared with production: the per-type codecs
//! it dispatches to (`Component::apply_update_payload` through
//! `lookup_component`, `apply_update_payload_for_type`) and the well-known
//! type table, which define each component's encoding.

use std::collections::BTreeMap;

use openmls::messages::proposals::AppDataUpdateOperation;
use tls_codec::{Deserialize, VLBytes};
use xmtp_cryptography::secret::Zeroizing;
use xmtp_mls_group_keys::group_keys::GROUP_ROOT_BYTES;
use xmtp_mls_tls_collections::{
    tls_map::{TlsMap, TlsMapDelta, TlsMapMutation},
    tls_set::{TlsSet, TlsSetDelta, TlsSetMutation},
};
use xmtp_proto::xmtp::mls::message_contents::ComponentType;

use crate::{
    app_data::{
        component_id::ComponentId,
        component_registry::ComponentRegistry,
        component_source::{
            ComponentSourceError, MAX_FIELD_ELEMENT_BYTES, MAX_FIELD_SNAPSHOT_BYTES, component_type,
        },
        components::type_dispatch::apply_update_payload_for_type,
        registry_table::lookup_component,
    },
    inbox_id::InboxId,
};

/// Each touched component's state after a proposal sequence: its bytes, or
/// `None` once removed.
pub(super) type States = BTreeMap<ComponentId, Option<Zeroizing<Vec<u8>>>>;

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
    let state = match operation {
        AppDataUpdateOperation::Update(payload) => Some(apply_app_data_update_payload(
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
        Some(component) => component.apply_update_payload(payload, old_value)?,
        None => {
            apply_update_payload_for_type(id, resolved_type(id, registry)?, payload, old_value)?
        }
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

fn resolved_type(
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
