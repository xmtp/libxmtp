//! Type-aware dispatch for `AppDataUpdate` proposals against a
//! component whose `Component` impl this client does not have.
//!
//! The receive-side relaxation path in
//! `xmtp_mls::groups::app_data::component_source` calls
//! [`lookup_component`](super::super::registry_table::lookup_component)
//! first; on a hit the per-id `Component` impl handles everything. On a
//! miss the caller looks the component up in the on-dict
//! [`ComponentRegistry`](crate::app_data::component_registry::ComponentRegistry),
//! pulls the registered [`ComponentType`], and dispatches through this
//! module.
//!
//! The dispatch is by-type because every `Component` impl in
//! [`super::metadata_attributes`], [`super::inbox_id_set`], and
//! [`super::tls_map_components`] is a thin per-id wrapper over the
//! type-level encoding semantics. Decoding bytes for an unknown
//! `TlsSet<InboxId>` component is identical to decoding bytes for
//! `AdminListComponent` — only the policy applied at validation time
//! differs, and that's keyed off the registry entry. So the closed
//! universe of six [`ComponentType`] variants is the same as the closed
//! universe of dispatch arms here.

use openmls::messages::proposals::AppDataUpdateOperation;
use std::collections::HashMap;
use tls_codec::{Deserialize, Serialize, VLBytes};
use xmtp_proto::xmtp::mls::message_contents::ComponentType;

use crate::{
    app_data::{
        component_id::ComponentId,
        component_registry::ComponentOp,
        components::{
            inbox_id_set::{apply_inbox_id_set_delta, expand_inbox_id_set_changes},
            metadata_attributes::{apply_passthrough, decode_utf8, expand_passthrough},
            tls_map_components::{apply_tls_map_delta, expand_tls_map_changes},
        },
        typed::{ComponentTypedError, ExpandedComponentChange},
    },
    inbox_id::InboxId,
    tls_set::{TlsKeyHash, TlsSet, TlsSetDelta, TlsSetError, TlsSetMutation},
};

/// Apply an `AppDataUpdateOperation::Update` payload for a component
/// whose `Component` impl is not available on this client, using only
/// its registered [`ComponentType`].
///
/// `component_id` is carried through to error context — it does not
/// influence the dispatch, which is purely a function of `ty`. Bare
/// `tls_codec::Error`s leaking out of the per-type helpers get wrapped
/// with the id so log triage can pinpoint *which* unknown component
/// failed to decode, instead of just seeing a context-free codec
/// error.
pub fn apply_update_payload_for_type(
    component_id: ComponentId,
    ty: ComponentType,
    payload: &[u8],
    prior: Option<&[u8]>,
) -> Result<Vec<u8>, ComponentTypedError> {
    let result = match ty {
        ComponentType::Bytes => apply_passthrough(payload),
        ComponentType::String => {
            let _ = decode_utf8(component_id, payload)?;
            apply_passthrough(payload)
        }
        ComponentType::TlsSetInboxId => apply_inbox_id_set_delta(payload, prior),
        ComponentType::TlsSetBytes => apply_bytes_set_delta(payload, prior),
        ComponentType::TlsMapInboxIdBytes => apply_tls_map_delta::<InboxId>(payload, prior),
        ComponentType::TlsMapBytesBytes => apply_tls_map_delta::<VLBytes>(payload, prior),
        ComponentType::Unspecified => Err(ComponentTypedError::UnspecifiedType(component_id)),
    };
    result.map_err(|e| attach_component_id(component_id, ty, e, ApplyOrExpand::Apply))
}

/// Expand an `AppDataUpdateOperation` for a component whose `Component`
/// impl is not available on this client, using only its registered
/// [`ComponentType`].
pub fn expand_to_changes_for_type(
    component_id: ComponentId,
    ty: ComponentType,
    op: &AppDataUpdateOperation,
    prior: Option<&[u8]>,
) -> Result<Vec<ExpandedComponentChange>, ComponentTypedError> {
    let result = match ty {
        ComponentType::Bytes => expand_passthrough(op),
        ComponentType::String => {
            if let AppDataUpdateOperation::Update(payload) = op {
                let _ = decode_utf8(component_id, payload.as_slice())?;
            }
            expand_passthrough(op)
        }
        ComponentType::TlsSetInboxId => expand_inbox_id_set_changes(op, prior),
        ComponentType::TlsSetBytes => expand_bytes_set_changes(op, prior),
        ComponentType::TlsMapInboxIdBytes => expand_tls_map_changes::<InboxId>(op, prior),
        ComponentType::TlsMapBytesBytes => expand_tls_map_changes::<VLBytes>(op, prior),
        ComponentType::Unspecified => Err(ComponentTypedError::UnspecifiedType(component_id)),
    };
    result.map_err(|e| attach_component_id(component_id, ty, e, ApplyOrExpand::Expand))
}

#[derive(Clone, Copy)]
enum ApplyOrExpand {
    Apply,
    Expand,
}

impl std::fmt::Display for ApplyOrExpand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Apply => f.write_str("apply"),
            Self::Expand => f.write_str("expand"),
        }
    }
}

/// Re-wrap bare `tls_codec::Error`s — which lack component context —
/// into a [`ComponentTypedError::MalformedValue`] that carries the id
/// and a short description of where the failure happened. Other
/// variants (already-structured `MalformedValue`, set/map apply
/// errors, etc.) pass through unchanged.
fn attach_component_id(
    component_id: ComponentId,
    ty: ComponentType,
    err: ComponentTypedError,
    site: ApplyOrExpand,
) -> ComponentTypedError {
    match err {
        ComponentTypedError::TlsCodec(inner) => ComponentTypedError::MalformedValue {
            component_id,
            reason: format!("type-aware {site} ({ty:?}) tls codec error: {inner}"),
        },
        other => other,
    }
}

// =============================================================================
// TlsSet<VLBytes> apply/expand
// =============================================================================
//
// Mirrors `apply_inbox_id_set_delta` / `expand_inbox_id_set_changes` but
// keyed on `VLBytes`. No well-known `TlsSetBytes` component ships today;
// this exists so a future XMTP-defined or runtime-registered bytes-set
// component lands on old clients via the same delta-aware path the
// inbox-id-set components use.

fn apply_bytes_set_delta(
    payload: &[u8],
    prior: Option<&[u8]>,
) -> Result<Vec<u8>, ComponentTypedError> {
    let delta = TlsSetDelta::<VLBytes>::tls_deserialize_exact(payload)?;
    let mut set: TlsSet<VLBytes> = match prior {
        Some(bytes) => TlsSet::<VLBytes>::tls_deserialize_exact(bytes)?,
        None => TlsSet::new(),
    };
    set.apply_delta(delta)?;
    Ok(set.tls_serialize_detached()?)
}

fn expand_bytes_set_changes(
    op: &AppDataUpdateOperation,
    prior: Option<&[u8]>,
) -> Result<Vec<ExpandedComponentChange>, ComponentTypedError> {
    match op {
        AppDataUpdateOperation::Remove => Ok(vec![ExpandedComponentChange {
            op: ComponentOp::Delete,
            value: None,
        }]),
        AppDataUpdateOperation::Update(payload) => {
            let delta = TlsSetDelta::<VLBytes>::tls_deserialize_exact(payload.as_slice())?;

            // Same RemoveByHash resolution discipline as
            // `expand_inbox_id_set_changes`: build the hash index once,
            // only when the delta carries at least one `RemoveByHash`,
            // and only when prior bytes exist.
            let needs_index = delta
                .mutations
                .iter()
                .any(|m| matches!(m, TlsSetMutation::RemoveByHash(_)));
            let hash_index: Option<HashMap<TlsKeyHash, VLBytes>> = if needs_index {
                match prior {
                    Some(bytes) => {
                        let prior_set = TlsSet::<VLBytes>::tls_deserialize_exact(bytes)?;
                        let mut idx = HashMap::with_capacity(prior_set.len());
                        for key in prior_set.iter() {
                            if idx.insert(TlsKeyHash::of(key)?, key.clone()).is_some() {
                                return Err(ComponentTypedError::TlsSetApply(
                                    TlsSetError::DuplicateHash,
                                ));
                            }
                        }
                        Some(idx)
                    }
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
                        value: Some(key.as_slice().to_vec()),
                    }),
                    TlsSetMutation::Remove(key) => out.push(ExpandedComponentChange {
                        op: ComponentOp::Delete,
                        value: Some(key.as_slice().to_vec()),
                    }),
                    TlsSetMutation::RemoveByHash(target) => {
                        let resolved = hash_index
                            .as_ref()
                            .and_then(|idx| idx.get(&target))
                            .map(|key| key.as_slice().to_vec());
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_data::component_registry::ComponentOp;
    use crate::tls_map::{TlsMap, TlsMapDelta, TlsMapMutation};

    const UNKNOWN_ID: ComponentId = ComponentId::new(0x80FF);

    fn vl(bytes: &[u8]) -> VLBytes {
        VLBytes::new(bytes.to_vec())
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn apply_bytes_passthrough() {
        let new_bytes = apply_update_payload_for_type(
            UNKNOWN_ID,
            ComponentType::Bytes,
            b"opaque-bytes",
            Some(b"prior-bytes"),
        )
        .unwrap();
        assert_eq!(new_bytes, b"opaque-bytes");
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn apply_string_validates_utf8() {
        let ok = apply_update_payload_for_type(UNKNOWN_ID, ComponentType::String, b"hello", None)
            .unwrap();
        assert_eq!(ok, b"hello");

        // Invalid UTF-8 (lone continuation byte) is rejected. Keeps
        // old-client behavior consistent with what a typed
        // `String`-shaped `Component` impl would do.
        let invalid =
            apply_update_payload_for_type(UNKNOWN_ID, ComponentType::String, &[0xC3, 0x28], None);
        assert!(matches!(
            invalid,
            Err(ComponentTypedError::MalformedValue { .. })
        ));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn apply_tls_set_inbox_id_delta() {
        let id1 = InboxId::from_bytes([1u8; 32]);
        let delta = TlsSetDelta::<InboxId>::new().insert(id1);
        let payload = delta.tls_serialize_detached().unwrap();
        let new_bytes =
            apply_update_payload_for_type(UNKNOWN_ID, ComponentType::TlsSetInboxId, &payload, None)
                .unwrap();
        let set = TlsSet::<InboxId>::tls_deserialize_exact(&new_bytes).unwrap();
        assert_eq!(set.len(), 1);
        assert!(set.contains(&id1));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn apply_tls_set_bytes_delta() {
        let delta = TlsSetDelta::<VLBytes>::new().insert(vl(b"alpha"));
        let payload = delta.tls_serialize_detached().unwrap();
        let new_bytes =
            apply_update_payload_for_type(UNKNOWN_ID, ComponentType::TlsSetBytes, &payload, None)
                .unwrap();
        let set = TlsSet::<VLBytes>::tls_deserialize_exact(&new_bytes).unwrap();
        assert_eq!(set.len(), 1);
        assert!(set.contains(&vl(b"alpha")));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn apply_tls_map_inbox_id_bytes_delta() {
        let id1 = InboxId::from_bytes([2u8; 32]);
        let mut delta = TlsMapDelta::<InboxId, VLBytes>::new();
        delta.mutations.push(TlsMapMutation::Insert {
            key: id1,
            value: vl(b"v1"),
        });
        let payload = delta.tls_serialize_detached().unwrap();
        let new_bytes = apply_update_payload_for_type(
            UNKNOWN_ID,
            ComponentType::TlsMapInboxIdBytes,
            &payload,
            None,
        )
        .unwrap();
        let map = TlsMap::<InboxId, VLBytes>::tls_deserialize_exact(&new_bytes).unwrap();
        assert_eq!(map.len(), 1);
        assert_eq!(map.get(&id1).map(|v| v.as_slice()), Some(&b"v1"[..]));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn apply_tls_map_bytes_bytes_delta() {
        let mut delta = TlsMapDelta::<VLBytes, VLBytes>::new();
        delta.mutations.push(TlsMapMutation::Insert {
            key: vl(b"k"),
            value: vl(b"v"),
        });
        let payload = delta.tls_serialize_detached().unwrap();
        let new_bytes = apply_update_payload_for_type(
            UNKNOWN_ID,
            ComponentType::TlsMapBytesBytes,
            &payload,
            None,
        )
        .unwrap();
        let map = TlsMap::<VLBytes, VLBytes>::tls_deserialize_exact(&new_bytes).unwrap();
        assert_eq!(map.len(), 1);
        assert_eq!(map.get(&vl(b"k")).map(|v| v.as_slice()), Some(&b"v"[..]));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn unspecified_type_is_rejected() {
        let err = apply_update_payload_for_type(
            UNKNOWN_ID,
            ComponentType::Unspecified,
            b"whatever",
            None,
        )
        .unwrap_err();
        assert!(matches!(err, ComponentTypedError::UnspecifiedType(_)));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn expand_bytes_remove_yields_delete_no_value() {
        let changes = expand_to_changes_for_type(
            UNKNOWN_ID,
            ComponentType::Bytes,
            &AppDataUpdateOperation::Remove,
            None,
        )
        .unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].op, ComponentOp::Delete);
        assert!(changes[0].value.is_none());
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn expand_tls_set_bytes_insert_yields_per_element() {
        let delta = TlsSetDelta::<VLBytes>::new()
            .insert(vl(b"alpha"))
            .insert(vl(b"beta"));
        let payload = delta.tls_serialize_detached().unwrap();
        let op = AppDataUpdateOperation::Update(payload.into());
        let changes =
            expand_to_changes_for_type(UNKNOWN_ID, ComponentType::TlsSetBytes, &op, None).unwrap();
        assert_eq!(changes.len(), 2);
        assert_eq!(changes[0].op, ComponentOp::Insert);
        assert_eq!(changes[0].value.as_deref(), Some(&b"alpha"[..]));
        assert_eq!(changes[1].op, ComponentOp::Insert);
        assert_eq!(changes[1].value.as_deref(), Some(&b"beta"[..]));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn expand_tls_set_bytes_remove_by_hash_resolves_against_prior() {
        let key = vl(b"target");
        let mut prior_set = TlsSet::<VLBytes>::new();
        prior_set.insert(key.clone()).unwrap();
        let prior_bytes = prior_set.tls_serialize_detached().unwrap();

        let target_hash = TlsKeyHash::of(&key).unwrap();
        let delta = TlsSetDelta::<VLBytes>::new().remove_by_hash(target_hash);
        let payload = delta.tls_serialize_detached().unwrap();
        let op = AppDataUpdateOperation::Update(payload.into());

        let changes = expand_to_changes_for_type(
            UNKNOWN_ID,
            ComponentType::TlsSetBytes,
            &op,
            Some(&prior_bytes),
        )
        .unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].op, ComponentOp::Delete);
        assert_eq!(changes[0].value.as_deref(), Some(&b"target"[..]));
    }
}
