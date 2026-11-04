use super::*;
use crate::app_data::component_permissions::component_permissions;
use xmtp_proto::xmtp::mls::message_contents::metadata_policy::{AndCondition, AnyCondition};

fn allow() -> MetadataPolicyProto {
    MetadataPolicyProto {
        kind: Some(MetadataPolicyKind::Base(MetadataBasePolicy::Allow as i32)),
    }
}

fn deny() -> MetadataPolicyProto {
    MetadataPolicyProto {
        kind: Some(MetadataPolicyKind::Base(MetadataBasePolicy::Deny as i32)),
    }
}

fn admin_only() -> MetadataPolicyProto {
    MetadataPolicyProto {
        kind: Some(MetadataPolicyKind::Base(
            MetadataBasePolicy::AllowIfAdmin as i32,
        )),
    }
}

/// An `AndCondition` whose leaves are all `AllowIfAdmin`. The combinator
/// wrapper itself must still be rejected for constrained components,
/// regardless of how admin-y its contents are.
fn and_condition_admin_only() -> MetadataPolicyProto {
    MetadataPolicyProto {
        kind: Some(MetadataPolicyKind::AndCondition(AndCondition {
            policies: vec![admin_only(), admin_only()],
        })),
    }
}

/// An `AnyCondition` whose leaves are all `AllowIfAdmin`. Same rationale
/// as `and_condition_admin_only`.
fn any_condition_admin_only() -> MetadataPolicyProto {
    MetadataPolicyProto {
        kind: Some(MetadataPolicyKind::AnyCondition(AnyCondition {
            policies: vec![admin_only(), admin_only()],
        })),
    }
}

fn sample_meta() -> ComponentMetadata {
    new_component_metadata(
        component_permissions()
            .insert(allow())
            .update(admin_only())
            .delete(deny())
            .call(),
        ComponentType::Bytes,
    )
}

#[xmtp_common::test]
fn test_set_and_get() {
    let mut reg = ComponentRegistry::new();
    let id = ComponentId::GROUP_NAME;
    reg.set(id, sample_meta()).unwrap();
    let meta = reg.get(&id).unwrap().unwrap();
    assert_eq!(meta, sample_meta());
}

#[xmtp_common::test]
fn test_set_overwrites() {
    let mut reg = ComponentRegistry::new();
    let id = ComponentId::GROUP_NAME;
    reg.set(id, sample_meta()).unwrap();

    let new_meta = new_component_metadata(
        component_permissions()
            .insert(deny())
            .update(deny())
            .delete(deny())
            .call(),
        ComponentType::TlsMapBytesBytes,
    );
    reg.set(id, new_meta.clone()).unwrap();

    let got = reg.get(&id).unwrap().unwrap();
    assert_eq!(got, new_meta);
    assert_eq!(reg.len(), 1);
}

#[xmtp_common::test]
fn test_remove() {
    let mut reg = ComponentRegistry::new();
    let id = ComponentId::GROUP_NAME;
    reg.set(id, sample_meta()).unwrap();
    reg.remove(&id).unwrap();
    assert!(!reg.contains(&id));
}

#[xmtp_common::test]
fn test_remove_missing_returns_error() {
    let mut reg = ComponentRegistry::new();
    let result = reg.remove(&ComponentId::GROUP_NAME);
    assert!(result.is_err());
}

#[xmtp_common::test(unwrap_try = true)]
fn test_reject_hardcoded_set() {
    // Hardcoded components must NEVER have a registry entry. Their
    // permissions are enforced in code by `validate_component_write`,
    // and a stored entry would create a silent disagreement between the
    // registry and the actual enforcement path.
    let mut reg = ComponentRegistry::new();
    assert!(matches!(
        reg.set(ComponentId::COMPONENT_REGISTRY, sample_meta()),
        Err(ComponentRegistryError::HardcodedComponent(_))
    ));
    assert!(matches!(
        reg.set(ComponentId::SUPER_ADMIN_LIST, sample_meta()),
        Err(ComponentRegistryError::HardcodedComponent(_))
    ));
    assert!(reg.is_empty());
}

#[xmtp_common::test(unwrap_try = true)]
fn test_reject_hardcoded_remove() {
    // Even though hardcoded entries should never be in the registry, the
    // remove path still rejects them for defense in depth.
    let mut reg = ComponentRegistry::new();
    assert!(matches!(
        reg.remove(&ComponentId::COMPONENT_REGISTRY),
        Err(ComponentRegistryError::HardcodedComponent(_))
    ));
    assert!(matches!(
        reg.remove(&ComponentId::SUPER_ADMIN_LIST),
        Err(ComponentRegistryError::HardcodedComponent(_))
    ));
}

#[xmtp_common::test]
fn test_admin_list_accepts_admin_or_super_admin_policy() {
    let mut reg = ComponentRegistry::new();
    let meta = new_component_metadata(
        component_permissions()
            .insert(admin_only())
            .update(admin_only())
            .delete(admin_only())
            .call(),
        ComponentType::Bytes,
    );
    assert!(reg.set(ComponentId::ADMIN_LIST, meta).is_ok());
}

#[xmtp_common::test]
fn test_admin_list_accepts_super_admin_only_policy() {
    let mut reg = ComponentRegistry::new();
    let super_admin_only = MetadataPolicyProto {
        kind: Some(MetadataPolicyKind::Base(
            MetadataBasePolicy::AllowIfSuperAdmin as i32,
        )),
    };
    let meta = new_component_metadata(
        component_permissions()
            .insert(super_admin_only.clone())
            .update(super_admin_only.clone())
            .delete(super_admin_only)
            .call(),
        ComponentType::Bytes,
    );
    assert!(reg.set(ComponentId::ADMIN_LIST, meta).is_ok());
}

#[xmtp_common::test]
fn test_admin_list_rejects_allow_policy() {
    let mut reg = ComponentRegistry::new();
    let meta = new_component_metadata(
        component_permissions()
            .insert(allow())
            .update(allow())
            .delete(allow())
            .call(),
        ComponentType::Bytes,
    );
    assert!(matches!(
        reg.set(ComponentId::ADMIN_LIST, meta),
        Err(ComponentRegistryError::ConstrainedPolicyViolation(_))
    ));
}

#[xmtp_common::test(unwrap_try = true)]
fn test_admin_list_accepts_deny_policy() {
    let mut reg = ComponentRegistry::new();
    let meta = new_component_metadata(
        component_permissions()
            .insert(admin_only())
            .update(deny())
            .delete(admin_only())
            .call(),
        ComponentType::Bytes,
    );
    assert!(reg.set(ComponentId::ADMIN_LIST, meta).is_ok());
}

#[xmtp_common::test]
fn test_admin_list_rejects_mixed_invalid_policy() {
    let mut reg = ComponentRegistry::new();
    let meta = new_component_metadata(
        component_permissions()
            .insert(admin_only())
            .update(allow())
            .delete(admin_only())
            .call(),
        ComponentType::Bytes,
    );
    assert!(matches!(
        reg.set(ComponentId::ADMIN_LIST, meta),
        Err(ComponentRegistryError::ConstrainedPolicyViolation(_))
    ));
}

#[xmtp_common::test]
fn test_admin_list_rejects_and_condition_policy() {
    // Combinator policies must be rejected for constrained components
    // even when every leaf is admin-only — it's the wrapper itself that's
    // disallowed, not the contents. Locks in the explicit rejection in
    // `is_admin_or_super_admin_or_deny_policy` so a future refactor of
    // that match
    // can't silently let combinators through.
    let mut reg = ComponentRegistry::new();
    let meta = new_component_metadata(
        component_permissions()
            .insert(and_condition_admin_only())
            .update(admin_only())
            .delete(admin_only())
            .call(),
        ComponentType::Bytes,
    );
    assert!(matches!(
        reg.set(ComponentId::ADMIN_LIST, meta),
        Err(ComponentRegistryError::ConstrainedPolicyViolation(_))
    ));
}

#[xmtp_common::test]
fn test_admin_list_rejects_any_condition_policy() {
    // Same rationale as `test_admin_list_rejects_and_condition_policy`,
    // but exercising the `AnyCondition` branch on a different policy slot
    // so all three slots and both combinator variants are covered across
    // the constrained-component test cluster.
    let mut reg = ComponentRegistry::new();
    let meta = new_component_metadata(
        component_permissions()
            .insert(admin_only())
            .update(any_condition_admin_only())
            .delete(admin_only())
            .call(),
        ComponentType::Bytes,
    );
    assert!(matches!(
        reg.set(ComponentId::ADMIN_LIST, meta),
        Err(ComponentRegistryError::ConstrainedPolicyViolation(_))
    ));
}

#[xmtp_common::test]
// verifies: META-014
fn test_rejects_missing_permissions() {
    let mut reg = ComponentRegistry::new();
    // Construct ComponentMetadata directly (bypassing new_component_metadata)
    // because the helper doesn't allow `permissions: None` — that's exactly
    // the negative case we're testing here.
    let meta = ComponentMetadata {
        permissions: None,
        external_committer_permissions: None,
        component_type: ComponentType::Bytes as i32,
    };
    // Applies to ALL components, not just constrained ones.
    assert!(matches!(
        reg.set(ComponentId::GROUP_NAME, meta.clone()),
        Err(ComponentRegistryError::MissingPermissions(_))
    ));
    assert!(matches!(
        reg.set(ComponentId::ADMIN_LIST, meta),
        Err(ComponentRegistryError::MissingPermissions(_))
    ));
}

#[xmtp_common::test]
// verifies: META-014
fn test_rejects_missing_policy_field() {
    let mut reg = ComponentRegistry::new();
    let meta = new_component_metadata(
        ComponentPermissions {
            insert_policy: Some(allow()),
            update_policy: None, // missing
            delete_policy: Some(allow()),
        },
        ComponentType::Bytes,
    );
    // Applies to ALL components, not just constrained ones.
    assert!(matches!(
        reg.set(ComponentId::GROUP_NAME, meta),
        Err(ComponentRegistryError::MissingPolicyField(
            _,
            ComponentOp::Update
        ))
    ));
}

#[xmtp_common::test]
fn test_reject_reserved_set() {
    let mut reg = ComponentRegistry::new();
    assert!(matches!(
        reg.set(ComponentId::new(0xFF00), sample_meta()),
        Err(ComponentRegistryError::ReservedRange(_))
    ));
}

#[xmtp_common::test]
fn test_reject_invalid_id() {
    let mut reg = ComponentRegistry::new();
    assert!(matches!(
        reg.set(ComponentId::new(0x0001), sample_meta()),
        Err(ComponentRegistryError::InvalidComponentId(_))
    ));
}

#[xmtp_common::test]
fn test_app_range_allowed() {
    let mut reg = ComponentRegistry::new();
    let id = ComponentId::new(0xC000);
    reg.set(id, sample_meta()).unwrap();
    assert!(reg.contains(&id));
}

#[xmtp_common::test]
fn test_immutable_first_insert_allowed() {
    // Immutable components can be inserted once.
    let mut reg = ComponentRegistry::new();
    let id = ComponentId::new(0xBE00);
    reg.set(id, sample_meta()).unwrap();
    assert!(reg.contains(&id));
}

#[xmtp_common::test]
fn test_immutable_subsequent_set_rejected() {
    let mut reg = ComponentRegistry::new();
    let id = ComponentId::new(0xBE00);
    reg.set(id, sample_meta()).unwrap();
    // Second set on the same immutable id is rejected.
    assert!(matches!(
        reg.set(id, sample_meta()),
        Err(ComponentRegistryError::ImmutableComponent(_))
    ));
}

#[xmtp_common::test]
fn test_immutable_remove_rejected() {
    let mut reg = ComponentRegistry::new();
    let id = ComponentId::new(0xBE00);
    reg.set(id, sample_meta()).unwrap();
    assert!(matches!(
        reg.remove(&id),
        Err(ComponentRegistryError::ImmutableComponent(_))
    ));
    assert!(reg.contains(&id));
}

#[xmtp_common::test]
fn test_tls_round_trip() {
    let mut reg = ComponentRegistry::new();
    reg.set(ComponentId::GROUP_NAME, sample_meta()).unwrap();
    reg.set(
        ComponentId::GROUP_DESCRIPTION,
        new_component_metadata(
            component_permissions()
                .insert(deny())
                .update(deny())
                .delete(deny())
                .call(),
            ComponentType::Bytes,
        ),
    )
    .unwrap();

    let bytes = reg.to_bytes().unwrap();
    let restored = ComponentRegistry::from_bytes(&bytes).unwrap();
    assert_eq!(reg, restored);
}

#[xmtp_common::test]
fn test_iter() {
    let mut reg = ComponentRegistry::new();
    reg.set(ComponentId::GROUP_NAME, sample_meta()).unwrap();
    reg.set(ComponentId::GROUP_DESCRIPTION, sample_meta())
        .unwrap();

    let entries: Vec<_> = reg.iter().collect::<Result<Vec<_>, _>>().unwrap();
    assert_eq!(entries.len(), 2);
    assert!(entries[0].0 < entries[1].0);
}

#[xmtp_common::test]
fn test_empty_registry_round_trip() {
    let reg = ComponentRegistry::new();
    let bytes = reg.to_bytes().unwrap();
    let restored = ComponentRegistry::from_bytes(&bytes).unwrap();
    assert_eq!(reg, restored);
    assert!(restored.is_empty());
}

/// Assert the tolerance contract for one invalid entry: the load
/// succeeds, the entry is invisible to every read, it is reported
/// via `unrecognized_ids`, and `to_bytes` preserves it verbatim.
fn assert_tolerated(bytes: &[u8], id: ComponentId) {
    let reg = ComponentRegistry::from_bytes(bytes).unwrap();
    assert!(reg.get(&id).unwrap().is_none());
    assert!(!reg.contains(&id));
    assert_eq!(reg.len(), 0);
    assert!(reg.iter().next().is_none());
    assert_eq!(reg.unrecognized_ids().collect::<Vec<_>>(), vec![id]);
    // Round-trip preserves the raw entry byte-for-byte.
    assert_eq!(reg.to_bytes().unwrap(), bytes);
}

#[xmtp_common::test]
fn test_from_bytes_tolerates_out_of_space_id() {
    // An out-of-space key (0x0001) is preserved-but-invisible: it
    // must not poison the load, because the registry is what makes
    // every other commit on the group validatable.
    let id = ComponentId::new(0x0001);
    let bytes = raw_bytes_with_entry(id, sample_meta().encode_to_vec());
    assert_tolerated(&bytes, id);
}

#[xmtp_common::test]
fn test_from_bytes_tolerates_reserved_id() {
    // Reserved-range ids are future protocol slots — a newer
    // version allocating one must degrade to "unknown entry" on
    // this version, not "registry unloadable".
    let id = ComponentId::new(0xFF50);
    let bytes = raw_bytes_with_entry(id, sample_meta().encode_to_vec());
    assert_tolerated(&bytes, id);
}

/// Build a TLS-encoded `TlsMap<ComponentId, VLBytes>` snapshot
/// containing a single entry. Bypasses `ComponentRegistry::set` so
/// we can construct payloads that the public API would refuse to
/// produce.
fn raw_bytes_with_entry(id: ComponentId, value: Vec<u8>) -> Vec<u8> {
    let mut map: TlsMap<ComponentId, VLBytes> = TlsMap::new();
    map.insert(id, VLBytes::new(value)).unwrap();
    map.tls_serialize_detached().unwrap()
}

#[xmtp_common::test]
fn test_from_bytes_tolerates_hardcoded_id() {
    // A smuggled shadow entry for a hardcoded component stays
    // invisible — its permissions are enforced in code, so hiding
    // the entry preserves exactly the enforcement that matters.
    let bytes = raw_bytes_with_entry(
        ComponentId::COMPONENT_REGISTRY,
        sample_meta().encode_to_vec(),
    );
    assert_tolerated(&bytes, ComponentId::COMPONENT_REGISTRY);

    let bytes = raw_bytes_with_entry(ComponentId::SUPER_ADMIN_LIST, sample_meta().encode_to_vec());
    assert_tolerated(&bytes, ComponentId::SUPER_ADMIN_LIST);
}

#[xmtp_common::test]
fn test_from_bytes_tolerates_missing_permissions() {
    // A metadata entry whose permissions field is None is invisible
    // rather than fatal: writes to that component then fail the
    // deny-by-default `NoRegistryEntry` policy check, which is the
    // safe direction.
    let meta = ComponentMetadata {
        permissions: None,
        external_committer_permissions: None,
        component_type: ComponentType::Bytes as i32,
    };
    let bytes = raw_bytes_with_entry(ComponentId::GROUP_NAME, meta.encode_to_vec());
    assert_tolerated(&bytes, ComponentId::GROUP_NAME);
}

#[xmtp_common::test]
fn test_from_bytes_tolerates_missing_policy_field() {
    // Permissions present, but one of the three policy fields is missing.
    let meta = new_component_metadata(
        ComponentPermissions {
            insert_policy: Some(allow()),
            update_policy: None,
            delete_policy: Some(allow()),
        },
        ComponentType::Bytes,
    );
    let bytes = raw_bytes_with_entry(ComponentId::GROUP_NAME, meta.encode_to_vec());
    assert_tolerated(&bytes, ComponentId::GROUP_NAME);
}

#[xmtp_common::test]
fn test_from_bytes_tolerates_constrained_violation() {
    // An ADMIN_LIST entry with an Allow policy violates the
    // constrained-component invariant; hiding it means admin-list
    // writes deny until a super admin repairs the entry — instead
    // of every commit on the group failing to validate.
    let meta = new_component_metadata(
        component_permissions()
            .insert(allow())
            .update(allow())
            .delete(allow())
            .call(),
        ComponentType::Bytes,
    );
    let bytes = raw_bytes_with_entry(ComponentId::ADMIN_LIST, meta.encode_to_vec());
    assert_tolerated(&bytes, ComponentId::ADMIN_LIST);
}

#[xmtp_common::test]
fn test_reads_skip_entry_with_undecodable_value() {
    // A snapshot with one good entry and one whose value at a valid id
    // isn't decodable `ComponentMetadata`. The good entry stays fully
    // visible; the corrupt one is skipped by `iter`, invisible to `get`
    // (deny-by-default rather than a surfaced error), reported by
    // `unrecognized_ids`, and preserved verbatim by `to_bytes`.
    let mut map: TlsMap<ComponentId, VLBytes> = TlsMap::new();
    map.insert(
        ComponentId::GROUP_NAME,
        VLBytes::new(sample_meta().encode_to_vec()),
    )
    .unwrap();
    map.insert(
        ComponentId::GROUP_DESCRIPTION,
        VLBytes::new(vec![0xFF, 0xFF, 0xFF, 0xFF]),
    )
    .unwrap();
    let bytes = map.tls_serialize_detached().unwrap();

    let reg = ComponentRegistry::from_bytes(&bytes).unwrap();

    let entries: Vec<_> = reg.iter().collect::<Result<Vec<_>, _>>().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].0, ComponentId::GROUP_NAME);
    assert_eq!(
        reg.get(&ComponentId::GROUP_NAME).unwrap(),
        Some(sample_meta())
    );
    assert!(reg.get(&ComponentId::GROUP_DESCRIPTION).unwrap().is_none());
    assert_eq!(
        reg.unrecognized_ids().collect::<Vec<_>>(),
        vec![ComponentId::GROUP_DESCRIPTION]
    );
    assert_eq!(reg.to_bytes().unwrap(), bytes);
}

#[xmtp_common::test]
// verifies: META-016
fn test_from_bytes_tolerates_malformed_protobuf_value() {
    // The key is valid but the value bytes are not a parseable
    // ComponentMetadata (e.g. a future breaking re-encoding).
    // Preserved-but-invisible, like every other invalid entry.
    let bytes = raw_bytes_with_entry(ComponentId::GROUP_NAME, vec![0xFF, 0xFF, 0xFF, 0xFF]);
    assert_tolerated(&bytes, ComponentId::GROUP_NAME);
}

#[xmtp_common::test]
fn test_from_bytes_rejects_undecodable_outer_map() {
    // Entry-level tolerance never extends to the outer container:
    // truncated or non-canonical snapshot bytes are a hard error.
    let mut bytes = raw_bytes_with_entry(ComponentId::GROUP_NAME, sample_meta().encode_to_vec());
    bytes.truncate(bytes.len() - 1);
    assert!(matches!(
        ComponentRegistry::from_bytes(&bytes),
        Err(ComponentRegistryError::TlsCodecError(_))
    ));
}

#[xmtp_common::test]
// verifies: META-016
fn test_from_bytes_mixed_valid_and_invalid_entries() {
    // One valid entry, one reserved-range entry: the valid one is
    // fully readable, the invalid one is preserved-but-invisible,
    // and the round-trip loses neither.
    let reserved = ComponentId::new(0xFF00);
    let mut map: TlsMap<ComponentId, VLBytes> = TlsMap::new();
    map.insert(
        ComponentId::GROUP_NAME,
        VLBytes::new(sample_meta().encode_to_vec()),
    )
    .unwrap();
    map.insert(reserved, VLBytes::new(b"future-slot".to_vec()))
        .unwrap();
    let bytes = map.tls_serialize_detached().unwrap();

    let reg = ComponentRegistry::from_bytes(&bytes).unwrap();
    assert_eq!(reg.len(), 1);
    assert_eq!(
        reg.get(&ComponentId::GROUP_NAME).unwrap(),
        Some(sample_meta())
    );
    assert!(!reg.contains(&reserved));
    assert_eq!(reg.unrecognized_ids().collect::<Vec<_>>(), vec![reserved]);
    assert_eq!(reg.to_bytes().unwrap(), bytes);
}

/// The headline tolerance property end-to-end at the unit level: a
/// blob carrying one valid entry and one poisoned entry must load,
/// keep the healthy component fully usable — including the
/// per-element policy gate commit validation runs
/// ([`validate_component_write`]) — deny writes against the
/// poisoned component, and round-trip byte-identically so the
/// poison is never dropped.
///
/// The load/lookup/round-trip half deliberately overlaps
/// `test_from_bytes_mixed_valid_and_invalid_entries`; this test
/// adds the validation-gate half.
// TODO: the fuller end-to-end version — a poisoned
// COMPONENT_REGISTRY dict entry on a real group still validating
// unrelated commits through `ValidatedCommit::from_staged_commit`
// — belongs in xmtp_mls's group tests, where the commit pipeline
// exists.
#[xmtp_common::test(unwrap_try = true)]
// verifies: PERM-012
fn test_poisoned_registry_still_validates_unrelated_writes() {
    use crate::app_data::validation::{
        ActorAuthority, ComponentChange, ComponentPermissionError, validate_component_write,
    };

    // One valid GROUP_NAME entry plus one reserved-range entry
    // whose bytes aren't even valid ComponentMetadata — the shape
    // a newer protocol version (or a historical bad write) would
    // leave behind.
    let poisoned = ComponentId::new(0xFF20);
    let mut map: TlsMap<ComponentId, VLBytes> = TlsMap::new();
    map.insert(
        ComponentId::GROUP_NAME,
        VLBytes::new(sample_meta().encode_to_vec()),
    )?;
    map.insert(poisoned, VLBytes::new(vec![0xDE, 0xAD, 0xBE, 0xEF]))?;
    let bytes = map.tls_serialize_detached()?;

    // The load tolerates the poison...
    let reg = ComponentRegistry::from_bytes(&bytes)?;
    assert_eq!(reg.unrecognized_ids().collect::<Vec<_>>(), vec![poisoned]);

    // ...and the healthy entry stays fully usable: readable, and
    // its raw bytes still pass entry validation.
    assert_eq!(reg.get(&ComponentId::GROUP_NAME)?, Some(sample_meta()));
    ComponentRegistry::validate_entry(ComponentId::GROUP_NAME, &sample_meta().encode_to_vec())?;

    // The policy gate still allows a write to the healthy
    // component against the poisoned registry...
    let member = ActorAuthority {
        is_admin: false,
        is_super_admin: false,
    };
    let healthy_write = ComponentChange::builder()
        .component_id(ComponentId::GROUP_NAME)
        .op(ComponentOp::Insert)
        .actor(member)
        .build();
    validate_component_write(&healthy_write, &reg)?;

    // ...while a write against the poisoned component falls to
    // deny-by-default (the entry is preserved but invisible).
    let poisoned_write = ComponentChange::builder()
        .component_id(poisoned)
        .op(ComponentOp::Insert)
        .actor(member)
        .build();
    assert!(matches!(
        validate_component_write(&poisoned_write, &reg),
        Err(ComponentPermissionError::NoRegistryEntry(id)) if id == poisoned
    ));

    // And serialization round-trips byte-identically — the poison
    // is preserved, never dropped.
    assert_eq!(reg.to_bytes()?, bytes);
}

#[xmtp_common::test]
fn test_set_repairs_unrecognized_entry() {
    // A valid write to an id that currently holds an unrecognized
    // entry replaces it — `to_bytes` must serialize the repaired
    // value, not resurrect the broken bytes.
    let broken = ComponentMetadata {
        permissions: None,
        external_committer_permissions: None,
        component_type: ComponentType::Bytes as i32,
    };
    let bytes = raw_bytes_with_entry(ComponentId::GROUP_NAME, broken.encode_to_vec());
    let mut reg = ComponentRegistry::from_bytes(&bytes).unwrap();
    assert_eq!(reg.unrecognized_ids().count(), 1);

    reg.set(ComponentId::GROUP_NAME, sample_meta()).unwrap();
    assert_eq!(reg.unrecognized_ids().count(), 0);
    assert_eq!(
        reg.get(&ComponentId::GROUP_NAME).unwrap(),
        Some(sample_meta())
    );

    let round_tripped = ComponentRegistry::from_bytes(&reg.to_bytes().unwrap()).unwrap();
    assert_eq!(round_tripped.unrecognized_ids().count(), 0);
    assert_eq!(
        round_tripped.get(&ComponentId::GROUP_NAME).unwrap(),
        Some(sample_meta())
    );
}

/// An immutable-range entry this build cannot read may be a newer
/// client's valid write. Write-once must hold for it, or a local write
/// would replace the group's entry because `contains` hides it.
#[xmtp_common::test]
// verifies: META-015
fn test_set_refuses_unrecognized_immutable_entry() {
    let id = ComponentId::new(0xFD00);
    let bytes = raw_bytes_with_entry(id, vec![0xFF, 0xFF]);
    let mut reg = ComponentRegistry::from_bytes(&bytes).unwrap();
    assert!(!reg.contains(&id));
    assert!(matches!(
        reg.set(id, sample_meta()),
        Err(ComponentRegistryError::ImmutableComponent(_))
    ));
    assert_eq!(reg.to_bytes().unwrap(), bytes);
}

/// Reconciliation asks whether the group holds any entry for an ID, so
/// `contains_raw` must see an unreadable entry that `contains` hides.
#[xmtp_common::test]
// verifies: META-067
fn test_contains_raw_sees_unrecognized_entry() {
    let id = ComponentId::new(0xC001);
    let reg = ComponentRegistry::from_bytes(&raw_bytes_with_entry(id, vec![0xFF])).unwrap();
    assert!(!reg.contains(&id));
    assert!(reg.contains_raw(&id));
    assert!(!reg.contains_raw(&ComponentId::new(0xC002)));
}

#[xmtp_common::test]
fn test_remove_deletes_unrecognized_entry() {
    // Repair-by-delete: removing a modifiable id that only exists
    // as an unrecognized entry drops the preserved bytes.
    let bytes = raw_bytes_with_entry(ComponentId::GROUP_NAME, vec![0xFF, 0xFF]);
    let mut reg = ComponentRegistry::from_bytes(&bytes).unwrap();
    assert_eq!(reg.unrecognized_ids().count(), 1);

    reg.remove(&ComponentId::GROUP_NAME).unwrap();
    assert_eq!(reg.unrecognized_ids().count(), 0);
    let empty = ComponentRegistry::new();
    assert_eq!(reg.to_bytes().unwrap(), empty.to_bytes().unwrap());
}

#[xmtp_common::test]
fn test_preserves_component_type() {
    let mut reg = ComponentRegistry::new();
    let meta = new_component_metadata(
        component_permissions()
            .insert(allow())
            .update(allow())
            .delete(deny())
            .call(),
        ComponentType::TlsMapBytesBytes,
    );
    reg.set(ComponentId::GROUP_MEMBERSHIP, meta).unwrap();

    let got = reg.get(&ComponentId::GROUP_MEMBERSHIP).unwrap().unwrap();
    assert_eq!(got.component_type, ComponentType::TlsMapBytesBytes as i32);
}
