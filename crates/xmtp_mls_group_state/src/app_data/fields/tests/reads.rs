//! Field snapshot tests: descriptors, lookups and typed reads.

use super::*;

/// The listing holds the valid application entries and the registered
/// public well-known fields, in ID order. A well-known field's type
/// is the protocol's even when its registry tag disagrees; an application
/// field's type is its registry tag, not the catalogue's; an unknown tag
/// is `Unknown`; internal and malformed entries, and entries missing a
/// member policy, are not fields.
// verifies: META-069
#[xmtp_common::test(unwrap_try = true)]
fn fields_describe_the_committed_registry() {
    use MetadataComponentType as T;
    let dictionary = group();
    let fields = snapshot(&dictionary);
    let described: Vec<_> = fields
        .fields()
        .iter()
        .map(|f| (f.field.clone(), f.component_type, f.is_user_field()))
        .collect();
    let string_map = T::Map {
        key_type: MetadataKeyType::InboxId,
        value_type: MetadataScalarType::String,
    };
    let bytes_map = T::Map {
        key_type: MetadataKeyType::InboxId,
        value_type: MetadataScalarType::Bytes,
    };
    assert_eq!(
        described,
        vec![
            (MetadataFieldRef::GROUP_NAME, T::String, false),
            (MetadataFieldRef::GROUP_DESCRIPTION, T::String, false),
            (MetadataFieldRef::USER_DISPLAY_NAME, string_map, true),
            (named(STATUS, "GROUP_NAME"), T::String, false),
            (MetadataFieldRef::new(SCORES), bytes_map, true),
            (MetadataFieldRef::new(FUTURE), T::Unknown { tag: 99 }, false),
            (
                MetadataFieldRef::new(TAGS),
                T::Set {
                    key_type: MetadataKeyType::InboxId
                },
                false
            ),
            (
                MetadataFieldRef::new(LINKS),
                T::Map {
                    key_type: MetadataKeyType::Bytes,
                    value_type: MetadataScalarType::Bytes
                },
                false
            ),
        ]
    );
    assert!(
        fields
            .fields()
            .iter()
            .all(|f| f.permissions == permissions(MetadataBasePolicy::Allow))
    );
    assert!(
        FieldSnapshot::new(None, &catalogue())
            .unwrap()
            .fields()
            .is_empty()
    );
}

/// A descriptor's permissions are the committed entry's, not the
/// catalogue's.
// verifies: META-069
#[xmtp_common::test(unwrap_try = true)]
fn permissions_come_from_the_registry() {
    let mut raw = ComponentMetadata::decode(entry(tag(ComponentType::String)).as_slice()).unwrap();
    raw.permissions = Some(permissions(MetadataBasePolicy::AllowIfAdmin));
    let dictionary = dictionary(&[(STATUS, raw.encode_to_vec())], &[]);
    let fields = snapshot(&dictionary);
    assert_eq!(
        fields.fields()[0].permissions,
        permissions(MetadataBasePolicy::AllowIfAdmin)
    );
}

/// A name lookup prefers the well-known field when an application field
/// has the same name, and finds nothing for an unlisted name.
// verifies: META-069
#[xmtp_common::test(unwrap_try = true)]
fn name_lookup_prefers_the_well_known_field() {
    let dictionary = group();
    let fields = snapshot(&dictionary);
    assert_eq!(
        fields.field("GROUP_NAME").map(|f| &f.field),
        Some(&MetadataFieldRef::GROUP_NAME)
    );
    assert_eq!(
        fields.field("USER_DISPLAY_NAME").map(|f| &f.field),
        Some(&MetadataFieldRef::USER_DISPLAY_NAME)
    );
    assert!(fields.field("MIN_SUPPORTED_PROTOCOL_VERSION").is_none());
    assert!(fields.field("GROUP_IMAGE").is_none());
    assert!(fields.field("nothing").is_none());
}

/// A ref resolves by ID alone: another name or none reads the same
/// field, and a batch record carries the listed ref.
// verifies: META-070
#[xmtp_common::test(unwrap_try = true)]
fn refs_resolve_by_id() {
    let dictionary = group();
    let fields = snapshot(&dictionary);
    let team = Some(MetadataValue::Scalar(string("Team")));
    for field in [
        MetadataFieldRef::GROUP_NAME,
        MetadataFieldRef::new(ComponentId::GROUP_NAME),
        named(ComponentId::GROUP_NAME, "STATUS"),
    ] {
        assert_eq!(fields.value(&field).unwrap(), team);
        assert_eq!(
            fields.values(std::slice::from_ref(&field)).unwrap(),
            vec![MetadataFieldValue {
                field: MetadataFieldRef::GROUP_NAME,
                value: team.clone(),
            }]
        );
    }
    // The catalogue names STATUS "GROUP_NAME", but a ref by that name
    // with STATUS's ID still reads STATUS.
    assert_eq!(fields.value(&named(STATUS, "GROUP_NAME")).unwrap(), None);
}

/// A batch read returns one record per ref in request order, typed by
/// the field, with a missing value absent. An unlisted ref, an internal
/// field, and an unknown type are errors.
// verifies: META-070
#[xmtp_common::test(unwrap_try = true)]
fn values_are_typed_and_in_request_order() {
    let dictionary = group();
    let fields = snapshot(&dictionary);
    let batch = fields
        .values(&[
            MetadataFieldRef::new(SCORES),
            MetadataFieldRef::GROUP_DESCRIPTION,
            MetadataFieldRef::new(TAGS),
            MetadataFieldRef::USER_DISPLAY_NAME,
        ])
        .unwrap();
    let values: Vec<_> = batch
        .into_iter()
        .map(|v| (v.field.component_id, v.value))
        .collect();
    assert_eq!(
        values,
        vec![
            (
                SCORES,
                Some(MetadataValue::Map(vec![MapEntry {
                    key: FieldKey::InboxId(inbox(0xA)),
                    value: FieldValue::Bytes(vec![7]),
                }]))
            ),
            (ComponentId::GROUP_DESCRIPTION, None),
            (
                TAGS,
                Some(MetadataValue::Set(vec![FieldKey::InboxId(inbox(0xB))]))
            ),
            (
                ComponentId::USER_DISPLAY_NAME,
                Some(MetadataValue::Map(vec![
                    MapEntry {
                        key: FieldKey::InboxId(inbox(0xA)),
                        value: string("Alix"),
                    },
                    MapEntry {
                        key: FieldKey::InboxId(inbox(0xC)),
                        value: string("Carol"),
                    },
                ]))
            ),
        ]
    );
    assert!(fields.values(&[]).unwrap().is_empty());
    for (field, id) in [
        (MetadataFieldRef::new(BROKEN), BROKEN),
        (MetadataFieldRef::GROUP_IMAGE, ComponentId::GROUP_IMAGE),
        (
            MetadataFieldRef::new(ComponentId::MIN_SUPPORTED_PROTOCOL_VERSION),
            ComponentId::MIN_SUPPORTED_PROTOCOL_VERSION,
        ),
    ] {
        assert!(matches!(
            fields.values(&[MetadataFieldRef::GROUP_NAME, field]),
            Err(FieldError::UnknownField(unknown)) if unknown == id
        ));
    }
    assert!(matches!(
        fields.value(&MetadataFieldRef::new(FUTURE)),
        Err(FieldError::UnsupportedType {
            component_id: FUTURE,
            tag: 99
        })
    ));
}

/// A map read takes a key of the field's key type and returns the
/// value under it, or none.
// verifies: META-070
#[xmtp_common::test(unwrap_try = true)]
fn map_value_reads_one_key() {
    let dictionary = group();
    let fields = snapshot(&dictionary);
    let names = MetadataFieldRef::USER_DISPLAY_NAME;
    assert_eq!(
        fields
            .map_value(&names, &FieldKey::InboxId(inbox(0xC)))
            .unwrap(),
        Some(string("Carol"))
    );
    assert_eq!(
        fields
            .map_value(&names, &FieldKey::InboxId(inbox(0xB)))
            .unwrap(),
        None
    );
    assert_eq!(
        fields
            .map_value(&MetadataFieldRef::new(LINKS), &FieldKey::Bytes(vec![1]))
            .unwrap(),
        None
    );
    for (field, key) in [
        (names.clone(), FieldKey::Bytes(vec![0xA; 32])),
        (MetadataFieldRef::new(LINKS), FieldKey::InboxId(inbox(0xA))),
        (MetadataFieldRef::GROUP_NAME, FieldKey::Bytes(vec![])),
        (MetadataFieldRef::new(TAGS), FieldKey::InboxId(inbox(0xB))),
    ] {
        assert!(matches!(
            fields.map_value(&field, &key),
            Err(FieldError::TypeMismatch(id)) if id == field.component_id
        ));
    }
    assert!(matches!(
        fields.map_value(&MetadataFieldRef::new(FUTURE), &FieldKey::Bytes(vec![])),
        Err(FieldError::UnsupportedType { .. })
    ));
}

/// User data defaults to every user field and every current member,
/// each with a list (empty when it holds nothing). Explicit inboxes may
/// name a former member; empty filters select nothing. Explicit fields
/// must be user fields in the group, each named once.
// verifies: META-072
#[xmtp_common::test(unwrap_try = true)]
fn user_data_selects_fields_and_inboxes() {
    let dictionary = group();
    let fields = snapshot(&dictionary);
    let value = |field: &MetadataFieldRef, value| UserFieldValue {
        field: field.clone(),
        value,
    };
    let scores = MetadataFieldRef::new(SCORES);
    let names = MetadataFieldRef::USER_DISPLAY_NAME;
    assert_eq!(
        fields.user_data(None, None).unwrap(),
        BTreeMap::from([
            (
                inbox(0xA),
                vec![
                    value(&names, string("Alix")),
                    value(&scores, FieldValue::Bytes(vec![7])),
                ]
            ),
            (inbox(0xB), vec![]),
        ])
    );
    assert_eq!(
        fields
            .user_data(
                Some(std::slice::from_ref(&scores)),
                Some(&[inbox(0xC), inbox(0xA)])
            )
            .unwrap(),
        BTreeMap::from([
            (inbox(0xA), vec![value(&scores, FieldValue::Bytes(vec![7]))]),
            (inbox(0xC), vec![]),
        ])
    );
    assert_eq!(
        fields.user_data(None, Some(&[inbox(0xC)])).unwrap(),
        BTreeMap::from([(inbox(0xC), vec![value(&names, string("Carol"))])])
    );
    assert_eq!(
        fields.user_data(Some(&[]), None).unwrap(),
        BTreeMap::from([(inbox(0xA), vec![]), (inbox(0xB), vec![])])
    );
    assert!(fields.user_data(None, Some(&[])).unwrap().is_empty());
    assert!(matches!(
        fields.user_data(Some(&[MetadataFieldRef::GROUP_NAME]), None),
        Err(FieldError::NotUserField(ComponentId::GROUP_NAME))
    ));
    assert!(matches!(
        fields.user_data(Some(&[MetadataFieldRef::new(LINKS)]), None),
        Err(FieldError::NotUserField(LINKS))
    ));
    assert!(matches!(
        fields.user_data(Some(&[MetadataFieldRef::new(BROKEN)]), None),
        Err(FieldError::UnknownField(BROKEN))
    ));
    assert!(matches!(
        fields.user_data(Some(&[scores.clone(), named(SCORES, "scores")]), None),
        Err(FieldError::DuplicateField(SCORES))
    ));
}

/// A user field whose stored value does not decode under its committed
/// type, as after a registry update re-types it, yields no user data
/// instead of failing the read of every other field. A single read of
/// it still reports the malformed value.
// verifies: META-072
#[xmtp_common::test(unwrap_try = true)]
fn user_data_skips_undecodable_fields() {
    let dictionary = dictionary(
        &[
            (
                ComponentId::GROUP_MEMBERSHIP,
                entry(tag(ComponentType::TlsMapInboxIdBytes)),
            ),
            (
                ComponentId::USER_DISPLAY_NAME,
                entry(tag(ComponentType::TlsMapInboxIdString)),
            ),
            (STATUS, entry(tag(ComponentType::TlsMapInboxIdString))),
        ],
        &[
            (
                ComponentId::GROUP_MEMBERSHIP,
                inbox_map(&[(inbox(0xA), &[1])]),
            ),
            (
                ComponentId::USER_DISPLAY_NAME,
                inbox_map(&[(inbox(0xA), b"Alix")]),
            ),
            (STATUS, inbox_map(&[(inbox(0xA), &[0xFF])])),
        ],
    );
    let fields = snapshot(&dictionary);
    let status = named(STATUS, "GROUP_NAME");
    let names = UserFieldValue {
        field: MetadataFieldRef::USER_DISPLAY_NAME,
        value: string("Alix"),
    };
    assert_eq!(
        fields.user_data(None, None)?,
        BTreeMap::from([(inbox(0xA), vec![names])])
    );
    assert_eq!(
        fields.user_data(Some(std::slice::from_ref(&status)), None)?,
        BTreeMap::from([(inbox(0xA), vec![])])
    );
    assert!(fields.value(&status).is_err());
}

/// In a DM the default inboxes are the `DM_MEMBERS` pair, whatever the
/// membership map holds.
// verifies: META-072
#[xmtp_common::test(unwrap_try = true)]
fn user_data_defaults_to_the_dm_pair() {
    let dictionary = dictionary(
        &[(
            ComponentId::USER_DISPLAY_NAME,
            entry(tag(ComponentType::TlsMapInboxIdString)),
        )],
        &[
            (
                ComponentId::DM_MEMBERS,
                inbox_set(&[inbox(0xA), inbox(0xD)]),
            ),
            (
                ComponentId::GROUP_MEMBERSHIP,
                inbox_map(&[(inbox(0xA), b"")]),
            ),
        ],
    );
    let data = snapshot(&dictionary).user_data(None, None).unwrap();
    assert_eq!(
        data.into_keys().collect::<Vec<_>>(),
        vec![inbox(0xA), inbox(0xD)]
    );
}
