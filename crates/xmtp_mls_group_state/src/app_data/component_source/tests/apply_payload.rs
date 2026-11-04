//! Component source tests: applying app-data update payloads.

use super::*;

// --- apply_app_data_update_payload -------------------------------------

/// Applying a `GROUP_ROOT_KEY` write keeps only a 32-byte value.
// verifies: META-010
#[xmtp_common::test(unwrap_try = true)]
fn applying_a_group_root_requires_32_bytes() {
    let apply = |len: usize| {
        apply_app_data_update_payload(
            ComponentId::GROUP_ROOT_KEY,
            &vec![7; len],
            None,
            &empty_registry(),
        )
    };
    assert_eq!(*apply(32)?, vec![7; 32]);
    for len in [0, 31, 33] {
        assert!(
            matches!(
                apply(len),
                Err(ComponentSourceError::MalformedComponentValue { .. })
            ),
            "a {len}-byte root was applied"
        );
    }
}

#[xmtp_common::test]
fn test_apply_bytes_payload_returns_payload_verbatim() {
    let payload = b"new_name";
    let new_value =
        apply_app_data_update_payload(ComponentId::GROUP_NAME, payload, None, &empty_registry())
            .unwrap();
    assert_eq!(*new_value, payload);
}

#[xmtp_common::test]
fn test_apply_bytes_payload_ignores_old_value() {
    // Full replacement — the old value is irrelevant.
    let new_value = apply_app_data_update_payload(
        ComponentId::GROUP_DESCRIPTION,
        b"replacement",
        Some(b"old_description"),
        &empty_registry(),
    )
    .unwrap();
    assert_eq!(*new_value, b"replacement");
}

#[xmtp_common::test]
fn test_apply_admin_list_insert_against_none() {
    // Apply an insert delta against a group that has no prior admin list.
    // The synthesized full value should be a TlsSet with the one inbox id.
    let inbox = fake_inbox_id(0x44);
    let insert_payload =
        inbox_set_delta(TlsSetMutation::Insert(InboxId::from_hex(&inbox).unwrap()));

    let new_bytes = apply_app_data_update_payload(
        ComponentId::ADMIN_LIST,
        &insert_payload,
        None,
        &empty_registry(),
    )
    .unwrap();

    let set = TlsSet::<InboxId>::tls_deserialize_exact(&new_bytes).unwrap();
    assert_eq!(set.len(), 1);
    assert!(set.contains(&fake_inbox(0x44)));
}

#[xmtp_common::test]
fn test_apply_admin_list_insert_against_existing_set() {
    // Build a prior set with one entry, then apply a delta that inserts a
    // second entry. Confirm both entries are present in the new value.
    let alice = fake_inbox_id(0x01);
    let bob = fake_inbox_id(0x02);

    let prior = encode_inbox_id_set(std::slice::from_ref(&alice)).unwrap();
    let insert_payload = inbox_set_delta(TlsSetMutation::Insert(InboxId::from_hex(&bob).unwrap()));

    let new_bytes = apply_app_data_update_payload(
        ComponentId::ADMIN_LIST,
        &insert_payload,
        Some(&prior),
        &empty_registry(),
    )
    .unwrap();

    let set = TlsSet::<InboxId>::tls_deserialize_exact(&new_bytes).unwrap();
    assert_eq!(set.len(), 2);
    assert!(set.contains(&fake_inbox(0x01)));
    assert!(set.contains(&fake_inbox(0x02)));
}

#[xmtp_common::test]
fn test_apply_admin_list_remove_against_existing_set() {
    let alice = fake_inbox_id(0x01);
    let bob = fake_inbox_id(0x02);

    let prior = encode_inbox_id_set(&[alice.clone(), bob.clone()]).unwrap();
    let remove_payload =
        inbox_set_delta(TlsSetMutation::Remove(InboxId::from_hex(&alice).unwrap()));

    let new_bytes = apply_app_data_update_payload(
        ComponentId::ADMIN_LIST,
        &remove_payload,
        Some(&prior),
        &empty_registry(),
    )
    .unwrap();

    let set = TlsSet::<InboxId>::tls_deserialize_exact(&new_bytes).unwrap();
    assert_eq!(set.len(), 1);
    assert!(set.contains(&fake_inbox(0x02)));
    assert!(!set.contains(&fake_inbox(0x01)));
}

#[xmtp_common::test]
fn test_apply_super_admin_list_delta() {
    let owner = fake_inbox_id(0xAA);
    let new_sa = fake_inbox_id(0xBB);

    let prior = encode_inbox_id_set(std::slice::from_ref(&owner)).unwrap();
    let add_payload = inbox_set_delta(TlsSetMutation::Insert(InboxId::from_hex(&new_sa).unwrap()));

    let new_bytes = apply_app_data_update_payload(
        ComponentId::SUPER_ADMIN_LIST,
        &add_payload,
        Some(&prior),
        &empty_registry(),
    )
    .unwrap();

    let set = TlsSet::<InboxId>::tls_deserialize_exact(&new_bytes).unwrap();
    assert_eq!(set.len(), 2);
    assert!(set.contains(&fake_inbox(0xAA)));
    assert!(set.contains(&fake_inbox(0xBB)));
}

#[xmtp_common::test]
fn test_apply_admin_list_malformed_payload_returns_tls_codec_error() {
    // Garbage bytes that aren't a valid TlsSetDelta — exercises the
    // wire-format decode failure path that ProcessMessageWithAppDataError
    // ::AppDataDecode wraps in production. The receiver-side
    // `process_message_with_app_data` propagates this through the new
    // GroupMessageProcessingError::OpenMlsProcessMessageWithAppData
    // variant rather than masking it as `FoundAppDataUpdateProposal`.
    let err = apply_app_data_update_payload(
        ComponentId::ADMIN_LIST,
        &[0xff, 0xff, 0xff, 0xff],
        None,
        &empty_registry(),
    )
    .unwrap_err();
    assert!(
        matches!(err, ComponentSourceError::TlsCodec(_)),
        "got {err:?}"
    );
}

#[xmtp_common::test]
fn test_apply_admin_list_malformed_old_value_returns_tls_codec_error() {
    // The delta payload is well-formed, but the old_value isn't a
    // valid TlsSet — also a TlsCodec error, just from the other side.
    let inbox = fake_inbox_id(0x55);
    let payload = inbox_set_delta(TlsSetMutation::Insert(InboxId::from_hex(&inbox).unwrap()));
    let err = apply_app_data_update_payload(
        ComponentId::ADMIN_LIST,
        &payload,
        Some(&[0xde, 0xad, 0xbe, 0xef]),
        &empty_registry(),
    )
    .unwrap_err();
    assert!(
        matches!(err, ComponentSourceError::TlsCodec(_)),
        "got {err:?}"
    );
}

#[xmtp_common::test]
fn test_apply_immutable_first_insert_allowed() {
    // Bootstrap-shape: an `Update(payload)` against an immutable
    // component with no prior value is the bootstrap commit's
    // first-write path. Apply must store the payload bytes
    // verbatim — the bootstrap validator's byte-compare catches a
    // peer that crafts a malicious initial value, so the apply
    // layer doesn't need its own decode/check step.
    let bytes = apply_app_data_update_payload(
        ComponentId::CONVERSATION_TYPE,
        b"seed",
        None,
        &empty_registry(),
    )
    .unwrap();
    assert_eq!(*bytes, b"seed");
}

#[xmtp_common::test]
fn test_apply_immutable_overwrite_rejected() {
    // Steady-state: an `Update(payload)` against an immutable
    // component that already has a prior value must fail. This is
    // the only path Byzantine peers have to mutate immutables
    // post-bootstrap, and the apply layer is the gatekeeper.
    let err = apply_app_data_update_payload(
        ComponentId::CONVERSATION_TYPE,
        b"junk",
        Some(b"prior"),
        &empty_registry(),
    )
    .unwrap_err();
    assert!(matches!(err, ComponentSourceError::ImmutableUpdate(_)));
}

#[xmtp_common::test]
fn test_apply_component_registry_delta_against_empty() {
    // Bootstrap shape: a `TlsMapDelta<ComponentId, VLBytes>` of
    // all-`Insert` mutations applied against an empty map produces
    // a materialized snapshot containing those entries. Values must
    // be structurally valid `ComponentMetadata` — registry deltas
    // are entry-validated at apply.
    let id_a = ComponentId::GROUP_NAME;
    let id_b = ComponentId::GROUP_DESCRIPTION;
    let meta_a = registry_with(id_a, ComponentType::String)
        .get(&id_a)
        .unwrap()
        .unwrap()
        .encode_to_vec();
    let meta_b = registry_with(id_b, ComponentType::Bytes)
        .get(&id_b)
        .unwrap()
        .unwrap()
        .encode_to_vec();
    let delta = TlsMapDelta::<ComponentId, VLBytes>::new()
        .insert(id_a, VLBytes::new(meta_a.clone()))
        .insert(id_b, VLBytes::new(meta_b.clone()));
    let payload = delta.tls_serialize_detached().unwrap();

    let new_bytes = apply_app_data_update_payload(
        ComponentId::COMPONENT_REGISTRY,
        &payload,
        None,
        &empty_registry(),
    )
    .unwrap();
    let map = TlsMap::<ComponentId, VLBytes>::tls_deserialize_exact(&new_bytes).unwrap();
    assert_eq!(map.len(), 2);
    assert_eq!(
        map.get(&id_a).map(|v| v.as_slice()),
        Some(meta_a.as_slice())
    );
    assert_eq!(
        map.get(&id_b).map(|v| v.as_slice()),
        Some(meta_b.as_slice())
    );
}

#[xmtp_common::test]
fn test_apply_group_membership_delta_against_existing_map() {
    // Post-bootstrap shape: an `Update` mutation applied on top of
    // a prior `TlsMap<InboxId, VLBytes>` snapshot produces a new
    // snapshot with the updated value.
    let alice = fake_inbox(0xAA);
    let bob = fake_inbox(0xBB);
    let mut prior: TlsMap<InboxId, VLBytes> = TlsMap::new();
    prior.set(alice, VLBytes::new(vec![0x01]));
    prior.set(bob, VLBytes::new(vec![0x02]));
    let prior_bytes = prior.tls_serialize_detached().unwrap();

    let delta = TlsMapDelta::<InboxId, VLBytes>::new()
        .update(alice, VLBytes::new(vec![0x99]))
        .delete(bob);
    let payload = delta.tls_serialize_detached().unwrap();

    let new_bytes = apply_app_data_update_payload(
        ComponentId::GROUP_MEMBERSHIP,
        &payload,
        Some(&prior_bytes),
        &empty_registry(),
    )
    .unwrap();
    let map = TlsMap::<InboxId, VLBytes>::tls_deserialize_exact(&new_bytes).unwrap();
    assert_eq!(map.len(), 1);
    assert_eq!(
        map.get(&alice).map(|v| v.as_slice()),
        Some([0x99].as_slice())
    );
    assert!(!map.contains_key(&bob));
}

#[xmtp_common::test]
fn test_apply_map_component_malformed_delta_returns_codec_error() {
    // Garbage bytes that aren't a valid TlsMapDelta surface as a
    // TLS-codec error, same shape as the set-component path.
    let err = apply_app_data_update_payload(
        ComponentId::COMPONENT_REGISTRY,
        &[0xff, 0xff, 0xff, 0xff],
        None,
        &empty_registry(),
    )
    .unwrap_err();
    assert!(
        matches!(err, ComponentSourceError::TlsCodec(_)),
        "got {err:?}"
    );
}

#[xmtp_common::test]
fn test_apply_map_component_apply_failure_surfaces_apply_error() {
    // A delta that updates a key not present in the prior snapshot
    // fails at apply time — surfaced as `TlsMapApply(KeyNotFound)`.
    let alice = fake_inbox(0x01);
    let delta = TlsMapDelta::<InboxId, VLBytes>::new().update(alice, VLBytes::new(vec![0x42]));
    let payload = delta.tls_serialize_detached().unwrap();
    let err = apply_app_data_update_payload(
        ComponentId::GROUP_MEMBERSHIP,
        &payload,
        None,
        &empty_registry(),
    )
    .unwrap_err();
    assert!(
        matches!(
            err,
            ComponentSourceError::TlsMapApply(TlsMapError::KeyNotFound)
        ),
        "got {err:?}"
    );
}

// --- Unknown-component tolerance via type-aware dispatch ---------------
//
// No per-id `Component` impl exists for these ids on this client; the
// sender shipped a newer release. Old clients look up the
// `ComponentType` registered for the id in the on-dict
// [`ComponentRegistry`] and route the payload through the
// type-level decoder. The seven `ComponentType` variants cover the
// wire-format universe, so any future well-known or
// application-range component lands convergently — including
// `TlsSet` / `TlsMap` deltas, which previously needed a per-id
// impl to apply correctly.

#[xmtp_common::test]
fn test_apply_unknown_id_with_bytes_registry_entry_stores_payload() {
    // Bytes shape: opaque passthrough, registry entry supplies the
    // type tag so the dispatch knows *not* to try a TLS-delta
    // decode (which would corrupt the bytes).
    let id = ComponentId::new(0xC123);
    let registry = registry_with(id, ComponentType::Bytes);
    let new_value = apply_app_data_update_payload(id, b"opaque", None, &registry).unwrap();
    assert_eq!(*new_value, b"opaque");
}

#[xmtp_common::test]
fn test_apply_unknown_id_with_string_registry_entry_validates_utf8() {
    // String shape: payload must be valid UTF-8. Bad bytes surface
    // as `MalformedComponentValue` rather than silently corrupting
    // the dict.
    let id = ComponentId::new(0xC222);
    let registry = registry_with(id, ComponentType::String);
    let ok = apply_app_data_update_payload(id, b"hello", None, &registry).unwrap();
    assert_eq!(*ok, b"hello");
    let err = apply_app_data_update_payload(id, &[0xC3, 0x28], None, &registry).unwrap_err();
    assert!(matches!(
        err,
        ComponentSourceError::MalformedComponentValue { .. }
    ));
}

#[xmtp_common::test]
fn test_apply_unknown_id_with_tls_set_inbox_id_applies_delta() {
    // The whole point of registry-typed dispatch: a new
    // `TlsSet<InboxId>` component lands as a typed delta apply, not
    // as an opaque blob replacement — old and new clients converge
    // on the same `TlsSet` snapshot byte-for-byte.
    let id = ComponentId::new(0xC333);
    let registry = registry_with(id, ComponentType::TlsSetInboxId);
    let bob = fake_inbox(0x02);
    let delta = TlsSetDelta::<InboxId>::new().insert(bob);
    let payload = delta.tls_serialize_detached().unwrap();
    let new_bytes = apply_app_data_update_payload(id, &payload, None, &registry).unwrap();
    let set = TlsSet::<InboxId>::tls_deserialize_exact(&new_bytes).unwrap();
    assert_eq!(set.len(), 1);
    assert!(set.contains(&bob));
}

#[xmtp_common::test]
fn test_apply_unknown_id_with_no_registry_entry_rejected() {
    // Deny-by-default: no per-id impl AND no registry entry means
    // we have no type to decode against. Reject rather than store
    // bytes whose shape we can't reason about — the alternative
    // would fork the dict the moment a typed client did know the
    // shape.
    let err =
        apply_app_data_update_payload(ComponentId::new(0xC456), b"x", None, &empty_registry())
            .unwrap_err();
    assert!(matches!(err, ComponentSourceError::UnknownComponent(_)));
}

/// Immutability is enforced range-by-id, not per-`Component`-impl,
/// so an unknown id sitting in the XMTP immutable range
/// (`0xBE00-0xBFFF`) still gets the bootstrap-style "first insert
/// allowed, subsequent overwrite rejected" contract — even when
/// the type-aware dispatcher (not a per-id `Component` impl) is
/// the path being exercised. Pins the reviewer's concern that the
/// tolerance branch might bypass immutability for newly-defined
/// immutable components that a NEWER release ships.
#[xmtp_common::test]
fn test_apply_unknown_id_in_xmtp_immutable_range_first_write_allowed() {
    let id = ComponentId::new(0xBE05); // XMTP immutable range
    assert!(id.is_immutable());
    let registry = registry_with(id, ComponentType::Bytes);
    let new_value = apply_app_data_update_payload(id, b"seed", None, &registry).unwrap();
    assert_eq!(*new_value, b"seed");
}

#[xmtp_common::test]
fn test_apply_unknown_id_in_xmtp_immutable_range_overwrite_rejected() {
    let id = ComponentId::new(0xBE05);
    assert!(id.is_immutable());
    let registry = registry_with(id, ComponentType::Bytes);
    let err = apply_app_data_update_payload(id, b"new", Some(b"old"), &registry).unwrap_err();
    assert!(matches!(err, ComponentSourceError::ImmutableUpdate(_)));
}

/// Same contract for the application immutable range
/// (`0xFD00-0xFEFF`) — overwrites of an unknown immutable
/// application component are rejected even though no per-id impl
/// exists on this client.
#[xmtp_common::test]
fn test_apply_unknown_id_in_app_immutable_range_overwrite_rejected() {
    let id = ComponentId::new(0xFD42); // app immutable range
    assert!(id.is_immutable());
    let registry = registry_with(id, ComponentType::Bytes);
    let err = apply_app_data_update_payload(id, b"new", Some(b"old"), &registry).unwrap_err();
    assert!(matches!(err, ComponentSourceError::ImmutableUpdate(_)));
}

#[xmtp_common::test]
fn test_apply_reserved_range_component_rejected_with_or_without_registry() {
    // 0xFF00+ is the reserved range. `ComponentRegistry::set`
    // refuses to insert reserved ids, so a registry entry can't
    // even be constructed — the apply path falls through to
    // `UnknownComponent`.
    let err =
        apply_app_data_update_payload(ComponentId::new(0xFF01), b"x", None, &empty_registry())
            .unwrap_err();
    assert!(matches!(err, ComponentSourceError::UnknownComponent(_)));
}

#[xmtp_common::test]
fn test_apply_out_of_range_component_rejected() {
    // Ids below 0x8000 are outside the AppData address space.
    let err =
        apply_app_data_update_payload(ComponentId::new(0x0042), b"x", None, &empty_registry())
            .unwrap_err();
    assert!(matches!(err, ComponentSourceError::UnknownComponent(_)));
}

#[xmtp_common::test]
fn test_apply_unknown_xmtp_range_id_with_registry_dispatches() {
    // Same path applies to the XMTP-defined range, not just the
    // app range: any 0x8000-0xBFFF id that lacks a per-id impl
    // routes through the registry.
    let id = ComponentId::new(0x8FFF);
    assert!(id.is_xmtp_range());
    let registry = registry_with(id, ComponentType::Bytes);
    let new_value = apply_app_data_update_payload(id, b"opaque", None, &registry).unwrap();
    assert_eq!(*new_value, b"opaque");
}
