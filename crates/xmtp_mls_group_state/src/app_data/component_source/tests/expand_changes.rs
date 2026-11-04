//! Component source tests: expanding app-data updates into changes.

use super::*;

// --- expand_app_data_update_to_changes ---------------------------------

/// Helper: wrap a `TlsSetDelta<InboxId>` payload in an
/// `AppDataUpdateOperation::Update(...)`.
fn update_op(delta: TlsSetDelta<InboxId>) -> AppDataUpdateOperation {
    AppDataUpdateOperation::Update(delta.tls_serialize_detached().unwrap().into())
}

#[xmtp_common::test]
fn test_expand_insert_surfaces_inbox_id_bytes() {
    let alice = fake_inbox(0x11);
    let delta: TlsSetDelta<InboxId> = TlsSetDelta {
        mutations: vec![TlsSetMutation::Insert(alice)],
    };
    let changes = expand_app_data_update_to_changes(
        ComponentId::ADMIN_LIST,
        &update_op(delta),
        None,
        &empty_registry(),
    )
    .unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].op, ComponentOp::Insert);
    assert_eq!(
        changes[0].value.as_deref(),
        Some(alice.as_bytes().as_slice())
    );
}

#[xmtp_common::test]
fn test_expand_remove_surfaces_inbox_id_bytes() {
    let alice = fake_inbox(0x22);
    let delta: TlsSetDelta<InboxId> = TlsSetDelta {
        mutations: vec![TlsSetMutation::Remove(alice)],
    };
    let changes = expand_app_data_update_to_changes(
        ComponentId::ADMIN_LIST,
        &update_op(delta),
        None,
        &empty_registry(),
    )
    .unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].op, ComponentOp::Delete);
    assert_eq!(
        changes[0].value.as_deref(),
        Some(alice.as_bytes().as_slice())
    );
}

#[xmtp_common::test]
fn test_expand_remove_by_hash_resolves_to_inbox_id_from_old_value() {
    // Prior set has alice + bob; RemoveByHash(hash(alice)) should
    // resolve back to alice's raw 32 bytes so the validator sees
    // *which* identity is being removed.
    let alice = fake_inbox(0xAA);
    let bob = fake_inbox(0xBB);
    let prior: TlsSet<InboxId> = [alice, bob].into_iter().collect();
    let old_bytes = prior.tls_serialize_detached().unwrap();

    let hash = TlsKeyHash::of(&alice).unwrap();
    let delta: TlsSetDelta<InboxId> = TlsSetDelta {
        mutations: vec![TlsSetMutation::RemoveByHash(hash)],
    };
    let changes = expand_app_data_update_to_changes(
        ComponentId::ADMIN_LIST,
        &update_op(delta),
        Some(&old_bytes),
        &empty_registry(),
    )
    .unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].op, ComponentOp::Delete);
    assert_eq!(
        changes[0].value.as_deref(),
        Some(alice.as_bytes().as_slice())
    );
}

#[xmtp_common::test]
fn test_expand_remove_by_hash_miss_surfaces_none_value() {
    // Prior set has alice; RemoveByHash targets bob's hash (not in set).
    // Expansion surfaces value: None — the CRDT apply step will later
    // reject with KeyNotFound. Expansion's job is reshape, not auth.
    let alice = fake_inbox(0x01);
    let bob = fake_inbox(0x02);
    let prior: TlsSet<InboxId> = [alice].into_iter().collect();
    let old_bytes = prior.tls_serialize_detached().unwrap();

    let hash = TlsKeyHash::of(&bob).unwrap();
    let delta: TlsSetDelta<InboxId> = TlsSetDelta {
        mutations: vec![TlsSetMutation::RemoveByHash(hash)],
    };
    let changes = expand_app_data_update_to_changes(
        ComponentId::ADMIN_LIST,
        &update_op(delta),
        Some(&old_bytes),
        &empty_registry(),
    )
    .unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].op, ComponentOp::Delete);
    assert!(changes[0].value.is_none());
}

#[xmtp_common::test]
fn test_expand_remove_by_hash_with_no_old_value_surfaces_none() {
    // No prior bytes → no set to search → every RemoveByHash misses.
    let hash = TlsKeyHash::of(&fake_inbox(0x33)).unwrap();
    let delta: TlsSetDelta<InboxId> = TlsSetDelta {
        mutations: vec![TlsSetMutation::RemoveByHash(hash)],
    };
    let changes = expand_app_data_update_to_changes(
        ComponentId::SUPER_ADMIN_LIST,
        &update_op(delta),
        None,
        &empty_registry(),
    )
    .unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].op, ComponentOp::Delete);
    assert!(changes[0].value.is_none());
}

#[xmtp_common::test]
fn test_expand_remove_by_hash_malformed_old_value_surfaces_codec_error() {
    // Our own dict bytes are corrupt — surface the decode error
    // loudly rather than silently degrading to value: None, since a
    // corrupt prior set signals a local-state bug, not a peer bug.
    let hash = TlsKeyHash::of(&fake_inbox(0x44)).unwrap();
    let delta: TlsSetDelta<InboxId> = TlsSetDelta {
        mutations: vec![TlsSetMutation::RemoveByHash(hash)],
    };
    let err = expand_app_data_update_to_changes(
        ComponentId::ADMIN_LIST,
        &update_op(delta),
        Some(&[0xde, 0xad, 0xbe, 0xef]),
        &empty_registry(),
    )
    .unwrap_err();
    assert!(
        matches!(err, ComponentSourceError::TlsCodec(_)),
        "got {err:?}"
    );
}

/// XIP §2.2: an unknown component id with `Update(payload)` and a
/// registered [`ComponentType::Bytes`] expands to a single Update
/// change carrying the payload, so registry-policy validation runs
/// against bytes a typed client would also accept opaquely.
#[xmtp_common::test]
fn test_expand_unknown_component_bytes_typed_emits_single_update() {
    let id = ComponentId::new(0x80FF);
    let registry = registry_with(id, ComponentType::Bytes);
    let changes = expand_app_data_update_to_changes(
        id,
        &AppDataUpdateOperation::Update(b"opaque".to_vec().into()),
        None,
        &registry,
    )
    .unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].op, ComponentOp::Update);
    assert_eq!(changes[0].value.as_deref(), Some(b"opaque".as_slice()));
}

/// Unknown id with `Remove` and any registered type expands to a
/// single Delete change — the wipe semantics are type-agnostic.
#[xmtp_common::test]
fn test_expand_unknown_component_remove_emits_single_delete() {
    let id = ComponentId::new(0x80FF);
    let registry = registry_with(id, ComponentType::Bytes);
    let changes =
        expand_app_data_update_to_changes(id, &AppDataUpdateOperation::Remove, None, &registry)
            .unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].op, ComponentOp::Delete);
    assert!(changes[0].value.is_none());
}

/// Type-aware dispatch is the whole point: an unknown
/// `TlsSet<InboxId>` component expands to a per-element change
/// list, not a single opaque blob — so the validator policy loop
/// inspects each `Insert` individually, the same as it would for a
/// known set-shaped component.
#[xmtp_common::test]
fn test_expand_unknown_component_tls_set_inbox_id_emits_per_element_changes() {
    let id = ComponentId::new(0x80FE);
    let registry = registry_with(id, ComponentType::TlsSetInboxId);
    let alice = fake_inbox(0x11);
    let bob = fake_inbox(0x22);
    let delta = TlsSetDelta::<InboxId>::new().insert(alice).insert(bob);
    let payload = delta.tls_serialize_detached().unwrap();
    let changes = expand_app_data_update_to_changes(
        id,
        &AppDataUpdateOperation::Update(payload.into()),
        None,
        &registry,
    )
    .unwrap();
    assert_eq!(changes.len(), 2);
    assert!(changes.iter().all(|c| c.op == ComponentOp::Insert));
}

/// Reserved-range ids are not tolerated. `ComponentRegistry::set`
/// rejects reserved ids at construction time, so a registry entry
/// can't even be built — the dispatcher falls through to the
/// `UnknownComponent` rejection.
#[xmtp_common::test]
fn test_expand_reserved_range_id_still_rejected() {
    let err = expand_app_data_update_to_changes(
        ComponentId::new(0xFF02),
        &AppDataUpdateOperation::Update(b"x".to_vec().into()),
        None,
        &empty_registry(),
    )
    .unwrap_err();
    assert!(matches!(err, ComponentSourceError::UnknownComponent(_)));
}

/// Deny-by-default: an unknown id with no registry entry is
/// rejected. Without a type tag we have no idea how to decode the
/// payload, and storing it opaquely would diverge from any future
/// typed client that DID know the shape.
#[xmtp_common::test]
fn test_expand_unknown_component_no_registry_entry_rejected() {
    let err = expand_app_data_update_to_changes(
        ComponentId::new(0x80FF),
        &AppDataUpdateOperation::Update(b"opaque".to_vec().into()),
        None,
        &empty_registry(),
    )
    .unwrap_err();
    assert!(matches!(err, ComponentSourceError::UnknownComponent(_)));
}

#[xmtp_common::test]
fn test_expand_skips_old_value_decode_when_no_remove_by_hash() {
    // Delta has only Insert/Remove — we should never touch old_value,
    // so even a garbage old_value must not fail the expansion.
    let alice = fake_inbox(0x55);
    let delta: TlsSetDelta<InboxId> = TlsSetDelta {
        mutations: vec![TlsSetMutation::Insert(alice)],
    };
    let changes = expand_app_data_update_to_changes(
        ComponentId::ADMIN_LIST,
        &update_op(delta),
        Some(&[0xff, 0xff, 0xff]), // intentionally malformed
        &empty_registry(),
    )
    .unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].op, ComponentOp::Insert);
}
