//! Size bounds on component values: which fields are bounded, and the checks an update
//! payload and a field snapshot must pass.

use super::*;

/// The type of component `id`, from two sources tried in order:
///
/// 1. In-code [`component_type`] mapping — covers well-known XMTP ids,
///    including those with no typed decoder (immutable seeds like
///    CREATOR_INBOX_ID — bootstrap-only first-write path).
/// 2. On-dict [`ComponentRegistry`] entry, which `validator` decodes once —
///    covers components a *newer* release ships that this client has never
///    heard of; the registry's `component_type` tag is the type oracle.
pub(crate) fn resolved_type(
    id: ComponentId,
    validator: &mut ComponentWriteValidator<'_>,
) -> Result<ComponentType, ComponentSourceError> {
    component_type(id).map_or_else(|| registered_component_type(id, validator), Ok)
}

/// Whether the field bounds cover component `id`: application components,
/// `USER_DISPLAY_NAME`, `GROUP_IMAGE`, and `COMPONENT_REGISTRY`, whose
/// application-range entries a DM participant may write.
pub(crate) fn is_field_bounded(id: ComponentId) -> bool {
    id.is_app_range()
        || [
            ComponentId::USER_DISPLAY_NAME,
            ComponentId::GROUP_IMAGE,
            ComponentId::COMPONENT_REGISTRY,
        ]
        .contains(&id)
}

/// Whether an update payload of component `id` is held to the element
/// bound: a field-bounded component, or `GROUP_MEMBERSHIP`, whose payload
/// entries are bounded so a stored entry never blocks commits.
pub(crate) fn has_payload_bound(id: ComponentId) -> bool {
    is_field_bounded(id) || id == ComponentId::GROUP_MEMBERSHIP
}

/// Reject an `AppDataUpdate::Update` payload for component `id` that names
/// a scalar, collection key, or collection value longer than
/// [`MAX_FIELD_ELEMENT_BYTES`], including one that a later mutation of the
/// same payload removes and the key of a removal. It reads only the payload,
/// so it holds whatever state the update is applied to.
// implements: META-068
pub(super) fn check_update_payload_bounds(
    id: ComponentId,
    payload: &[u8],
    validator: &mut ComponentWriteValidator<'_>,
) -> Result<(), ComponentSourceError> {
    if !has_payload_bound(id) {
        return Ok(());
    }
    check_payload_element_bound(id, resolved_type(id, validator)?, payload)
}

/// [`check_update_payload_bounds`] for a component whose type `ty` is
/// already resolved.
// implements: META-068
pub(crate) fn check_payload_element_bound(
    id: ComponentId,
    ty: ComponentType,
    payload: &[u8],
) -> Result<(), ComponentSourceError> {
    // A removal by hash names no key bytes.
    let longest = match ty {
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

/// The longest value that a map delta with fixed-size `K` keys inserts or
/// updates.
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

/// Reject `value`, the new bytes of component `id` of type `ty`, when a
/// scalar, collection key, or collection value is longer than
/// [`MAX_FIELD_ELEMENT_BYTES`] or a collection snapshot is longer than
/// [`MAX_FIELD_SNAPSHOT_BYTES`]. `InboxId` and `ComponentId` keys have a
/// fixed size, so only byte keys and values are measured element by element.
// implements: META-068
pub(crate) fn check_field_bounds(
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

/// The longest value of a map with fixed-size `K` keys.
fn longest_map_value<K: Ord>(value: &[u8]) -> Result<Option<usize>, tls_codec::Error>
where
    TlsMap<K, VLBytes>: Deserialize,
{
    Ok(TlsMap::<K, VLBytes>::tls_deserialize_exact(value)?
        .values()
        .map(|v| v.as_slice().len())
        .max())
}

/// Reject `len`, the longest scalar, key, or value written to component
/// `id`, when it is over [`MAX_FIELD_ELEMENT_BYTES`].
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

/// Look up the [`ComponentType`] registered for a component id in the
/// on-dict [`ComponentRegistry`]. Returns
/// [`ComponentSourceError::UnknownComponent`] when no registry entry
/// exists — the deny-by-default rule that keeps unrecognized payloads
/// from being applied opaquely.
fn registered_component_type(
    id: ComponentId,
    validator: &mut ComponentWriteValidator<'_>,
) -> Result<ComponentType, ComponentSourceError> {
    let meta = validator
        .entry(id)
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
