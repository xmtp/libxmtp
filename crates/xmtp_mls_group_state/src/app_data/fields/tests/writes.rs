//! Field snapshot tests: field writes and their resolution against current values.

use super::*;

fn update(write: FieldWrite) -> Vec<u8> {
    match write.operation {
        WriteOperation::Update(payload) => payload,
        other => panic!("expected an update, got {other:?}"),
    }
}

/// A write is encoded under the committed type: the protocol's for a
/// well-known field and the registry's for an application field, never
/// the catalogue's. A value, key, or mutation of the wrong shape, an
/// unknown type, and an unlisted field are refused.
// verifies: META-071
#[xmtp_common::test(unwrap_try = true)]
fn field_writes_follow_the_committed_type() {
    use ComponentMutation as M;
    let dictionary = group();
    let fields = snapshot(&dictionary);
    let status = MetadataFieldRef::new(STATUS);
    let write = fields
        .field_write(&status, &M::Replace(string("away")))
        .unwrap();
    assert_eq!(write.component_type, ComponentType::String);
    assert_eq!(update(write), b"away");

    let insert = M::MapDelta(vec![MapMutation::Insert(
        FieldKey::InboxId(inbox(0xB)),
        string("Bo"),
    )]);
    let write = fields
        .field_write(&MetadataFieldRef::USER_DISPLAY_NAME, &insert)
        .unwrap();
    assert_eq!(write.component_type, ComponentType::TlsMapInboxIdString);
    assert_eq!(
        update(write),
        TlsMapDelta::<InboxId, VLBytes>::new()
            .insert(inbox(0xB), b"Bo".as_slice().into())
            .tls_serialize_detached()
            .unwrap()
    );

    let set = M::SetDelta(vec![
        SetMutation::Insert(FieldKey::InboxId(inbox(0xA))),
        SetMutation::DeleteByHash([1; 32]),
    ]);
    let write = fields
        .field_write(&MetadataFieldRef::new(TAGS), &set)
        .unwrap();
    assert_eq!(
        update(write),
        TlsSetDelta::<InboxId>::new()
            .insert(inbox(0xA))
            .remove_by_hash(TlsKeyHash::from_bytes([1; 32]))
            .tls_serialize_detached()
            .unwrap()
    );
    assert_eq!(
        fields.field_write(&status, &M::Remove).unwrap().operation,
        WriteOperation::Remove
    );

    let bytes_name = M::MapDelta(vec![MapMutation::Update(
        FieldKey::InboxId(inbox(0xA)),
        FieldValue::Bytes(b"Alix".to_vec()),
    )]);
    for (field, mutation) in [
        (
            status.clone(),
            M::Replace(FieldValue::Bytes(b"away".to_vec())),
        ),
        (status.clone(), M::MapDelta(vec![])),
        (MetadataFieldRef::USER_DISPLAY_NAME, bytes_name),
        (
            MetadataFieldRef::new(SCORES),
            M::MapDelta(vec![MapMutation::Delete(FieldKey::Bytes(vec![0xA; 32]))]),
        ),
        (
            MetadataFieldRef::new(TAGS),
            M::SetDelta(vec![SetMutation::Delete(FieldKey::Bytes(vec![]))]),
        ),
        (MetadataFieldRef::new(LINKS), M::SetDelta(vec![])),
    ] {
        assert!(matches!(
            fields.field_write(&field, &mutation),
            Err(FieldError::TypeMismatch(id)) if id == field.component_id
        ));
    }
    for mutation in [M::Remove, M::Replace(FieldValue::Bytes(vec![]))] {
        assert!(matches!(
            fields.field_write(&MetadataFieldRef::new(FUTURE), &mutation),
            Err(FieldError::UnsupportedType {
                component_id: FUTURE,
                tag: 99
            })
        ));
    }
    assert!(matches!(
        fields.field_write(&MetadataFieldRef::GROUP_IMAGE, &M::Remove),
        Err(FieldError::UnknownField(ComponentId::GROUP_IMAGE))
    ));
}

/// Own user data updates name each user field once, with a value of its
/// map value type.
// verifies: META-073
#[xmtp_common::test(unwrap_try = true)]
fn user_data_writes_refuse_bad_updates() {
    let dictionary = group();
    let fields = snapshot(&dictionary);
    let set = |field: MetadataFieldRef, value| UserFieldUpdate { field, value };
    assert_eq!(
        fields
            .user_data_writes(&[
                set(MetadataFieldRef::USER_DISPLAY_NAME, Some(string("Bo"))),
                set(MetadataFieldRef::new(SCORES), None),
            ])
            .unwrap(),
        vec![
            FieldWrite {
                component_id: ComponentId::USER_DISPLAY_NAME,
                component_type: ComponentType::TlsMapInboxIdString,
                operation: WriteOperation::SetOwn(b"Bo".to_vec()),
            },
            FieldWrite {
                component_id: SCORES,
                component_type: ComponentType::TlsMapInboxIdBytes,
                operation: WriteOperation::ClearOwn,
            },
        ]
    );
    assert!(fields.user_data_writes(&[]).unwrap().is_empty());
    assert!(matches!(
        fields.user_data_writes(&[
            set(MetadataFieldRef::USER_DISPLAY_NAME, Some(string("Bo"))),
            set(named(ComponentId::USER_DISPLAY_NAME, "nickname"), None),
        ]),
        Err(FieldError::DuplicateField(ComponentId::USER_DISPLAY_NAME))
    ));
    assert!(matches!(
        fields.user_data_writes(&[set(MetadataFieldRef::GROUP_NAME, Some(string("x")))]),
        Err(FieldError::NotUserField(ComponentId::GROUP_NAME))
    ));
    assert!(matches!(
        fields.user_data_writes(&[set(MetadataFieldRef::new(SCORES), Some(string("7")))]),
        Err(FieldError::TypeMismatch(SCORES))
    ));
    assert!(matches!(
        fields.user_data_writes(&[set(
            MetadataFieldRef::USER_DISPLAY_NAME,
            Some(FieldValue::Bytes(b"Bo".to_vec()))
        )]),
        Err(FieldError::TypeMismatch(ComponentId::USER_DISPLAY_NAME))
    ));
    assert!(matches!(
        fields.user_data_writes(&[set(MetadataFieldRef::new(BROKEN), None)]),
        Err(FieldError::UnknownField(BROKEN))
    ));
}

fn own_write(id: ComponentId, ty: ComponentType, operation: WriteOperation) -> FieldWrite {
    FieldWrite {
        component_id: id,
        component_type: ty,
        operation,
    }
}

fn map_delta(delta: TlsMapDelta<InboxId, VLBytes>) -> AppDataUpdateOperation {
    AppDataUpdateOperation::Update(delta.tls_serialize_detached().unwrap().into())
}

/// An own-key write only ever names the writer's inbox: a set is an
/// Insert when the key is absent and an Update when present, a clear
/// is a Delete when present and nothing when absent.
// verifies: META-073
#[xmtp_common::test(unwrap_try = true)]
fn own_writes_resolve_against_current_values() {
    let dictionary = group();
    let fields = snapshot(&dictionary);
    let names = |operation| {
        own_write(
            ComponentId::USER_DISPLAY_NAME,
            ComponentType::TlsMapInboxIdString,
            operation,
        )
    };
    let scores = |operation| own_write(SCORES, ComponentType::TlsMapInboxIdBytes, operation);
    let resolve = |own, writes: &[FieldWrite]| {
        fields
            .resolve_writes(Some(&dictionary), own, writes)
            .unwrap()
    };
    let id = ComponentId::USER_DISPLAY_NAME;
    assert_eq!(
        resolve(inbox(0xA), &[names(WriteOperation::SetOwn(b"Al".to_vec()))]),
        vec![(
            id,
            map_delta(TlsMapDelta::new().update(inbox(0xA), b"Al".as_slice().into()))
        )]
    );
    assert_eq!(
        resolve(inbox(0xB), &[names(WriteOperation::SetOwn(b"Bo".to_vec()))]),
        vec![(
            id,
            map_delta(TlsMapDelta::new().insert(inbox(0xB), b"Bo".as_slice().into()))
        )]
    );
    assert_eq!(
        resolve(
            inbox(0xA),
            &[
                names(WriteOperation::ClearOwn),
                scores(WriteOperation::ClearOwn)
            ]
        ),
        vec![
            (id, map_delta(TlsMapDelta::new().delete(inbox(0xA)))),
            (SCORES, map_delta(TlsMapDelta::new().delete(inbox(0xA)))),
        ]
    );
    assert!(
        resolve(
            inbox(0xB),
            &[
                names(WriteOperation::ClearOwn),
                scores(WriteOperation::ClearOwn)
            ]
        )
        .is_empty()
    );
    assert!(resolve(inbox(0xB), &[]).is_empty());
    // Against an empty dictionary every set inserts.
    assert_eq!(
        fields
            .resolve_writes(
                None,
                inbox(0xA),
                &[names(WriteOperation::SetOwn(b"A".to_vec()))]
            )
            .unwrap(),
        vec![(
            id,
            map_delta(TlsMapDelta::new().insert(inbox(0xA), b"A".as_slice().into()))
        )]
    );
    assert!(matches!(
        fields.resolve_writes(
            Some(&dictionary),
            inbox(0xA),
            &[
                names(WriteOperation::ClearOwn),
                names(WriteOperation::ClearOwn)
            ]
        ),
        Err(FieldError::DuplicateField(ComponentId::USER_DISPLAY_NAME))
    ));
}

/// A write is checked against its current value before any commit: a
/// removal of an absent component is dropped, and a value the group
/// would reject, or a delta that no longer applies, is refused.
// verifies: META-071
#[xmtp_common::test(unwrap_try = true)]
fn writes_are_checked_against_current_values() {
    let dictionary = group();
    let fields = snapshot(&dictionary);
    let status = |operation| own_write(STATUS, ComponentType::String, operation);
    let resolve =
        |writes: &[FieldWrite]| fields.resolve_writes(Some(&dictionary), inbox(0xA), writes);
    assert!(
        resolve(&[status(WriteOperation::Remove)])
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        resolve(&[own_write(
            SCORES,
            ComponentType::TlsMapInboxIdBytes,
            WriteOperation::Remove
        )])
        .unwrap(),
        vec![(SCORES, AppDataUpdateOperation::Remove)]
    );
    assert!(matches!(
        resolve(&[status(WriteOperation::Update(vec![b'x'; 8193]))]),
        Err(FieldError::Component(_))
    ));
    assert!(matches!(
        resolve(&[status(WriteOperation::Update(vec![0xC3, 0x28]))]),
        Err(FieldError::Component(_))
    ));
    let insert_present = TlsMapDelta::<InboxId, VLBytes>::new()
        .insert(inbox(0xA), b"again".as_slice().into())
        .tls_serialize_detached()
        .unwrap();
    assert!(matches!(
        resolve(&[own_write(
            ComponentId::USER_DISPLAY_NAME,
            ComponentType::TlsMapInboxIdString,
            WriteOperation::Update(insert_present)
        )]),
        Err(FieldError::Component(_))
    ));
}

/// A write whose result equals the current value is dropped, so an
/// unchanged field never costs a commit or an epoch. The comparison is
/// on the applied bytes: an empty delta creates an absent map. The
/// operations that would carry the writes out keep them.
// verifies: META-071
#[xmtp_common::test(unwrap_try = true)]
fn unchanged_writes_are_dropped() {
    let dictionary = group();
    let fields = snapshot(&dictionary);
    let resolve = |writes: &[FieldWrite]| {
        fields
            .resolve_writes(Some(&dictionary), inbox(0xA), writes)
            .unwrap()
    };
    let name = |value: &[u8]| {
        own_write(
            ComponentId::GROUP_NAME,
            ComponentType::String,
            WriteOperation::Update(value.to_vec()),
        )
    };
    let empty_links = TlsMapDelta::<VLBytes, VLBytes>::new().tls_serialize_detached()?;
    let empty_scores = TlsMapDelta::<InboxId, VLBytes>::new().tls_serialize_detached()?;
    assert!(
        resolve(&[
            name(b"Team"),
            own_write(
                ComponentId::USER_DISPLAY_NAME,
                ComponentType::TlsMapInboxIdString,
                WriteOperation::SetOwn(b"Alix".to_vec()),
            ),
            own_write(
                SCORES,
                ComponentType::TlsMapInboxIdBytes,
                WriteOperation::Update(empty_scores),
            ),
        ])
        .is_empty()
    );
    assert_eq!(
        resolve(&[
            name(b"Teams"),
            own_write(
                LINKS,
                ComponentType::TlsMapBytesBytes,
                WriteOperation::Update(empty_links.clone()),
            ),
        ]),
        vec![
            (
                ComponentId::GROUP_NAME,
                AppDataUpdateOperation::Update(b"Teams".to_vec().into())
            ),
            (LINKS, AppDataUpdateOperation::Update(empty_links.into())),
        ]
    );
    assert_eq!(
        fields.write_operations(
            Some(&dictionary),
            inbox(0xA),
            &[
                name(b"Team"),
                own_write(
                    ComponentId::USER_DISPLAY_NAME,
                    ComponentType::TlsMapInboxIdString,
                    WriteOperation::SetOwn(b"Alix".to_vec()),
                ),
            ]
        )?,
        vec![
            (
                ComponentId::GROUP_NAME,
                AppDataUpdateOperation::Update(b"Team".to_vec().into())
            ),
            (
                ComponentId::USER_DISPLAY_NAME,
                map_delta(TlsMapDelta::new().update(inbox(0xA), b"Alix".as_slice().into()))
            ),
        ]
    );
}

/// An own-key write names an entry of an inbox-keyed map, so one queued
/// for any other shape is refused rather than applied as that shape's
/// payload, even when its type tag matches.
// verifies: META-073
#[xmtp_common::test(unwrap_try = true)]
fn own_writes_need_a_user_field() {
    let dictionary = group();
    let fields = snapshot(&dictionary);
    for (id, ty) in [
        (STATUS, ComponentType::String),
        (LINKS, ComponentType::TlsMapBytesBytes),
    ] {
        for operation in [
            WriteOperation::SetOwn(b"x".to_vec()),
            WriteOperation::ClearOwn,
        ] {
            assert!(matches!(
                fields.resolve_writes(
                    Some(&dictionary),
                    inbox(0xA),
                    &[own_write(id, ty, operation)]
                ),
                Err(FieldError::TypeMismatch(got)) if got == id
            ));
        }
    }
}

/// A write encoded under one snapshot is refused if its field changed
/// type or left the registry before it is resolved.
// verifies: META-071
#[xmtp_common::test(unwrap_try = true)]
fn stale_writes_are_refused() {
    let encoded = snapshot(&group())
        .field_write(
            &MetadataFieldRef::new(STATUS),
            &ComponentMutation::Replace(string("x")),
        )
        .unwrap();
    let retyped = dictionary(&[(STATUS, entry(tag(ComponentType::Bytes)))], &[]);
    assert!(matches!(
        snapshot(&retyped).resolve_writes(
            Some(&retyped),
            inbox(0xA),
            std::slice::from_ref(&encoded)
        ),
        Err(FieldError::TypeChanged {
            component_id: STATUS,
            expected: ComponentType::String,
            actual: MetadataComponentType::Bytes,
        })
    ));
    let removed = dictionary(&[], &[]);
    assert!(matches!(
        snapshot(&removed).resolve_writes(Some(&removed), inbox(0xA), &[encoded]),
        Err(FieldError::UnknownField(STATUS))
    ));
}

/// An entry this build cannot interpret neither blocks the snapshot nor
/// is touched: other fields still read and write, the entry keeps its
/// bytes, and a write names only its own component.
// verifies: META-016, META-017
#[xmtp_common::test(unwrap_try = true)]
fn uninterpreted_entries_are_left_alone() {
    let dictionary = group();
    let before = dictionary.clone();
    let fields = snapshot(&dictionary);
    let writes = fields
        .user_data_writes(&[UserFieldUpdate {
            field: MetadataFieldRef::USER_DISPLAY_NAME,
            value: Some(string("Bo")),
        }])
        .unwrap();
    let updates = fields
        .resolve_writes(Some(&dictionary), inbox(0xB), &writes)
        .unwrap();
    assert_eq!(
        updates.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        vec![ComponentId::USER_DISPLAY_NAME]
    );
    let _ = fields.values(&[MetadataFieldRef::new(FUTURE)]);
    assert_eq!(dictionary, before);
    assert_eq!(dictionary.get(&BROKEN.as_u16()), Some([1, 2, 3].as_slice()));
}
