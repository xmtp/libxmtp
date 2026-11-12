//! Component source tests: field and payload size bounds.

use super::*;

// --- field bounds ------------------------------------------------------
fn bytes(len: usize) -> VLBytes {
    VLBytes::new(vec![b'a'; len])
}

/// A scalar of an application component or `GROUP_IMAGE` may hold 8192
/// bytes and no more, so one writer cannot fill the commit envelope.
// verifies: META-068
#[xmtp_common::test(unwrap_try = true)]
fn bounded_scalar_holds_at_most_the_element_limit() {
    let app = ComponentId::new(0xC068);
    let registry = registry_with(app, ComponentType::Bytes);
    for id in [app, ComponentId::GROUP_IMAGE] {
        let at_limit = vec![0; MAX_FIELD_ELEMENT_BYTES];
        assert_eq!(
            *apply_app_data_update_payload(id, &at_limit, None, &registry)?,
            at_limit
        );
        let over = [0; MAX_FIELD_ELEMENT_BYTES + 1];
        assert!(matches!(
            apply_app_data_update_payload(id, &over, None, &registry),
            Err(ComponentSourceError::FieldBoundExceeded {
                component_id,
                len: 8193,
                max: MAX_FIELD_ELEMENT_BYTES,
            }) if component_id == id
        ));
    }
}

/// Each key and value of an application byte map, and each
/// `USER_DISPLAY_NAME` and `GROUP_MEMBERSHIP` value, may hold 8192 bytes
/// and no more.
// verifies: META-068
#[xmtp_common::test(unwrap_try = true)]
fn bounded_map_elements_hold_at_most_the_element_limit() {
    let app = ComponentId::new(0xC069);
    let registry = registry_with(app, ComponentType::TlsMapBytesBytes);
    let app_entry = |key: usize, value: usize| {
        let delta = TlsMapDelta::<VLBytes, VLBytes>::new().insert(bytes(key), bytes(value));
        apply_app_data_update_payload(
            app,
            &delta.tls_serialize_detached().unwrap(),
            None,
            &registry,
        )
    };
    let name = |len: usize| {
        let delta = TlsMapDelta::<InboxId, VLBytes>::new().insert(fake_inbox(1), bytes(len));
        apply_app_data_update_payload(
            ComponentId::USER_DISPLAY_NAME,
            &delta.tls_serialize_detached().unwrap(),
            None,
            &registry,
        )
    };
    let over = |result| {
        matches!(
            result,
            Err(ComponentSourceError::FieldBoundExceeded {
                len: 8193,
                max: MAX_FIELD_ELEMENT_BYTES,
                ..
            })
        )
    };

    app_entry(MAX_FIELD_ELEMENT_BYTES, MAX_FIELD_ELEMENT_BYTES)?;
    assert!(over(app_entry(MAX_FIELD_ELEMENT_BYTES + 1, 1)));
    assert!(over(app_entry(1, MAX_FIELD_ELEMENT_BYTES + 1)));
    name(MAX_FIELD_ELEMENT_BYTES)?;
    assert!(over(name(MAX_FIELD_ELEMENT_BYTES + 1)));
    let entry = |len: usize| {
        let delta = TlsMapDelta::<InboxId, VLBytes>::new().insert(fake_inbox(1), bytes(len));
        apply_app_data_update_payload(
            ComponentId::GROUP_MEMBERSHIP,
            &delta.tls_serialize_detached().unwrap(),
            None,
            &registry,
        )
    };
    entry(MAX_FIELD_ELEMENT_BYTES)?;
    assert!(over(entry(MAX_FIELD_ELEMENT_BYTES + 1)));
}

/// A serialized collection may hold 65536 bytes and no more, even when
/// every element is within its own limit.
// verifies: META-068
#[xmtp_common::test(unwrap_try = true)]
fn bounded_collection_holds_at_most_the_snapshot_limit() {
    let app = ComponentId::new(0xC06A);
    let registry = registry_with(app, ComponentType::TlsMapBytesBytes);
    // Eight 8000-byte entries plus one of `last` bytes.
    let write = |last: usize| {
        let delta = (0..8u8)
            .fold(TlsMapDelta::<VLBytes, VLBytes>::new(), |d, k| {
                d.insert(VLBytes::new(vec![k]), bytes(8000))
            })
            .insert(VLBytes::new(vec![8]), bytes(last));
        apply_app_data_update_payload(
            app,
            &delta.tls_serialize_detached().unwrap(),
            None,
            &registry,
        )
    };
    // The value length prefix is two bytes from 64 up, so the snapshot
    // grows one byte per value byte from there.
    let last = 64 + MAX_FIELD_SNAPSHOT_BYTES - write(64)?.len();

    assert_eq!(write(last)?.len(), MAX_FIELD_SNAPSHOT_BYTES);
    assert!(matches!(
        write(last + 1),
        Err(ComponentSourceError::FieldBoundExceeded {
            len: 65537,
            max: MAX_FIELD_SNAPSHOT_BYTES,
            ..
        })
    ));
}

/// A registry entry padded to `len` bytes by an unknown protobuf field,
/// which the registry decodes past and stores whole.
fn padded_registry_entry(len: usize) -> VLBytes {
    let id = ComponentId::new(0xC100);
    let mut entry = registry_with(id, ComponentType::Bytes)
        .get(&id)
        .unwrap()
        .unwrap()
        .encode_to_vec();
    // Field 15, length-delimited, with a two-byte length varint.
    let pad = len - entry.len() - 3;
    assert!((128..16384).contains(&pad));
    entry.extend([0x7A, pad as u8 | 0x80, (pad >> 7) as u8]);
    entry.resize(len, 0);
    VLBytes::new(entry)
}

/// A registry entry may hold 8192 bytes and no more, unknown protobuf
/// fields included: a DM participant may write application-range
/// entries, and the registry keeps their bytes whole.
// verifies: META-068
#[xmtp_common::test(unwrap_try = true)]
fn registry_entries_hold_at_most_the_element_limit() {
    let registry = ComponentRegistry::new();
    let insert = |len| {
        TlsMapDelta::<ComponentId, VLBytes>::new()
            .insert(ComponentId::new(0xC100), padded_registry_entry(len))
            .tls_serialize_detached()
            .unwrap()
    };
    let apply = |payload: &[u8]| {
        apply_app_data_update_payload(ComponentId::COMPONENT_REGISTRY, payload, None, &registry)
            .map(drop)
    };
    let check = |payload: &[u8]| {
        check_update_payload_bounds(
            ComponentId::COMPONENT_REGISTRY,
            payload,
            &mut ComponentWriteValidator::new(&registry),
        )
    };
    let exceeded = |result| {
        matches!(
            result,
            Err(ComponentSourceError::FieldBoundExceeded {
                len: 8193,
                max: MAX_FIELD_ELEMENT_BYTES,
                ..
            })
        )
    };

    let at_limit = insert(MAX_FIELD_ELEMENT_BYTES);
    check(&at_limit)?;
    apply(&at_limit)?;
    let over = insert(MAX_FIELD_ELEMENT_BYTES + 1);
    assert!(exceeded(check(&over)));
    assert!(exceeded(apply(&over)));
}

/// A serialized registry may hold 65536 bytes and no more, so repeated
/// in-bound inserts cannot grow every member's group context without
/// limit.
// verifies: META-068
#[xmtp_common::test(unwrap_try = true)]
fn registry_holds_at_most_the_snapshot_limit() {
    // Eight 8000-byte entries plus one of `last` bytes.
    let write = |last: usize| {
        let delta = (0..8)
            .fold(TlsMapDelta::<ComponentId, VLBytes>::new(), |d, k| {
                d.insert(ComponentId::new(0xC100 + k), padded_registry_entry(8000))
            })
            .insert(ComponentId::new(0xC108), padded_registry_entry(last));
        apply_app_data_update_payload(
            ComponentId::COMPONENT_REGISTRY,
            &delta.tls_serialize_detached().unwrap(),
            None,
            &ComponentRegistry::new(),
        )
    };
    // The entry's length prefixes are two bytes from 256 up, so the
    // snapshot grows one byte per entry byte from there.
    let last = 256 + MAX_FIELD_SNAPSHOT_BYTES - write(256)?.len();

    assert_eq!(write(last)?.len(), MAX_FIELD_SNAPSHOT_BYTES);
    assert!(matches!(
        write(last + 1),
        Err(ComponentSourceError::FieldBoundExceeded {
            component_id: ComponentId::COMPONENT_REGISTRY,
            len: 65537,
            max: MAX_FIELD_SNAPSHOT_BYTES,
        })
    ));
}

/// The bounds cover application components, `USER_DISPLAY_NAME`,
/// `GROUP_IMAGE`, and `COMPONENT_REGISTRY` only, and `GROUP_MEMBERSHIP`
/// entries but not its snapshot. Other XMTP components keep their own
/// limits.
// verifies: META-068
#[xmtp_common::test(unwrap_try = true)]
fn field_bounds_skip_other_xmtp_components() {
    let registered = ComponentId::new(0x80FF);
    let over = [b'a'; MAX_FIELD_ELEMENT_BYTES + 1];
    let registry = registry_with(registered, ComponentType::Bytes);
    apply_app_data_update_payload(registered, &over, None, &registry)?;
    apply_app_data_update_payload(ComponentId::GROUP_DESCRIPTION, &over, None, &registry)?;
    let members = (0..9).fold(TlsMapDelta::<InboxId, VLBytes>::new(), |delta, n| {
        delta.insert(fake_inbox(n), bytes(MAX_FIELD_ELEMENT_BYTES))
    });
    let snapshot = apply_app_data_update_payload(
        ComponentId::GROUP_MEMBERSHIP,
        &members.tls_serialize_detached()?,
        None,
        &registry,
    )?;
    assert!(snapshot.len() > MAX_FIELD_SNAPSHOT_BYTES);
}

/// A payload's element bound covers every byte key and value it names,
/// removal keys included: an oversized key can never match a stored one,
/// so admitting it only spends proposal storage. An element is bounded
/// even when a later mutation of the same payload removes it, so the
/// final value cannot hide it.
// verifies: META-068
#[xmtp_common::test(unwrap_try = true)]
fn payload_bounds_measure_every_named_element() {
    const LIMIT: usize = MAX_FIELD_ELEMENT_BYTES;
    const OVER: usize = LIMIT + 1;
    type Set = TlsSetDelta<VLBytes>;
    type Map = TlsMapDelta<VLBytes, VLBytes>;
    let apply = |id, ty, payload: Vec<u8>| {
        apply_app_data_update_payload(id, &payload, None, &registry_with(id, ty))
    };
    let set = ComponentId::new(0xC06B);
    let in_set = |build: fn(Set) -> Set| {
        let payload = build(Set::new()).tls_serialize_detached().unwrap();
        apply(set, ComponentType::TlsSetBytes, payload)
    };
    let map = ComponentId::new(0xC06C);
    let in_map = |build: fn(Map) -> Map| {
        let payload = build(Map::new()).tls_serialize_detached().unwrap();
        apply(map, ComponentType::TlsMapBytesBytes, payload)
    };

    for accepted in [
        in_set(|d| d.insert(bytes(LIMIT))),
        in_set(|d| d.insert(bytes(LIMIT)).remove(bytes(LIMIT))),
        in_map(|d| d.insert(bytes(LIMIT), bytes(1)).delete(bytes(LIMIT))),
    ] {
        accepted?;
    }
    for rejected in [
        in_set(|d| d.insert(bytes(OVER))),
        in_set(|d| d.remove(bytes(OVER))),
        in_set(|d| d.insert(bytes(OVER)).remove(bytes(OVER))),
        in_map(|d| d.update(bytes(OVER), bytes(1))),
        in_map(|d| d.update(bytes(1), bytes(OVER))),
        in_map(|d| d.delete(bytes(OVER))),
        in_map(|d| d.insert(bytes(1), bytes(OVER)).delete(bytes(1))),
        in_map(|d| d.insert(bytes(OVER), bytes(1)).delete(bytes(OVER))),
    ] {
        assert!(
            matches!(
                rejected,
                Err(ComponentSourceError::FieldBoundExceeded { len: OVER, .. })
            ),
            "{rejected:?}"
        );
    }
}
