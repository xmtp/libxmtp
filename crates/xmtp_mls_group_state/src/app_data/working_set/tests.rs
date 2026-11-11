use std::{borrow::Cow, collections::BTreeMap};

use openmls::messages::proposals::AppDataUpdateOperation;
use prost::Message;
use tls_codec::{Serialize, VLBytes};
use xmtp_mls_tls_collections::{
    tls_map::{TlsMap, TlsMapDelta, TlsMapError},
    tls_set::{TlsKeyHash, TlsSet, TlsSetDelta, TlsSetError},
};
use xmtp_proto::xmtp::mls::message_contents::{
    ComponentMetadata, ComponentType, MetadataPolicy,
    metadata_policy::{Kind, MetadataBasePolicy},
};

use super::{WorkingSet, counters};
use crate::{
    app_data::{
        component_id::ComponentId,
        component_permissions::component_permissions,
        component_registry::{ComponentRegistry, new_component_metadata},
        component_source::{
            ComponentSourceError, MAX_FIELD_SNAPSHOT_BYTES, expand_app_data_update_to_changes,
        },
        validation::count_validation_work,
    },
    inbox_id::InboxId,
};

mod differential;
mod reference;

/// Registry metadata with type tag `tag` that allows every write.
fn metadata(tag: i32) -> ComponentMetadata {
    let allow = || MetadataPolicy {
        kind: Some(Kind::Base(MetadataBasePolicy::Allow as i32)),
    };
    let permissions = component_permissions()
        .insert(allow())
        .update(allow())
        .delete(allow())
        .call();
    ComponentMetadata {
        component_type: tag,
        ..new_component_metadata(permissions, ComponentType::Bytes)
    }
}

/// A registry with an entry of each type tag.
fn registry(entries: &[(ComponentId, i32)]) -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    for (id, tag) in entries {
        registry.set(*id, metadata(*tag)).unwrap();
    }
    registry
}

fn update(payload: Vec<u8>) -> AppDataUpdateOperation {
    AppDataUpdateOperation::Update(payload.into())
}

/// An update of a bytes map that inserts each one-byte key with a value of
/// `len` bytes.
fn insert(entries: &[(u8, usize)]) -> AppDataUpdateOperation {
    let delta = entries
        .iter()
        .fold(TlsMapDelta::new(), |delta, &(key, len)| {
            delta.insert(VLBytes::new(vec![key]), VLBytes::new(vec![b'v'; len]))
        });
    update(delta.tls_serialize_detached().unwrap())
}

/// An update of an inbox-id set that inserts `inbox`.
fn insert_inbox(inbox: InboxId) -> Vec<u8> {
    TlsSetDelta::new()
        .insert(inbox)
        .tls_serialize_detached()
        .unwrap()
}

/// A proposal that leaves a bounded snapshot at 65,537 bytes fails, even
/// when a later proposal of the commit shrinks it below the bound.
// verifies: META-068
#[xmtp_common::test(unwrap_try = true)]
fn intermediate_snapshot_bound() {
    let id = ComponentId::new(0xC0A0);
    let registry = registry(&[(id, ComponentType::TlsMapBytesBytes as i32)]);
    let shrink = TlsMapDelta::<VLBytes, VLBytes>::new().delete(VLBytes::new(vec![0]));
    let proposals = [
        // Seven 8,004-byte entries and a 4-byte length: 56,032 bytes.
        (id, insert(&[0, 1, 2, 3, 4, 5, 6].map(|key| (key, 8000)))),
        // 8,196 and 1,309 more bytes: 65,537.
        (id, insert(&[(7, 8192), (8, 1305)])),
        (id, update(shrink.tls_serialize_detached()?)),
    ];

    let mut set = WorkingSet::new(Cow::Borrowed(&registry), |_| None);
    set.apply(id, &proposals[0].1)?;
    assert_eq!(set.snapshot_len(id), Some(56_032));
    let rejected = set.apply(id, &proposals[1].1);
    assert!(
        matches!(
            rejected,
            Err(ComponentSourceError::FieldBoundExceeded {
                len: 65_537,
                max: MAX_FIELD_SNAPSHOT_BYTES,
                ..
            })
        ),
        "{rejected:?}"
    );
    assert!(matches!(
        reference::apply_sequence(&registry, &BTreeMap::new(), &proposals),
        Err((
            1,
            ComponentSourceError::FieldBoundExceeded { len: 65_537, .. }
        ))
    ));
}

/// The snapshot bound admits a bounded snapshot of exactly 65,536 bytes
/// and fails one byte more, as the byte-level apply does.
// verifies: META-068
#[xmtp_common::test(unwrap_try = true)]
fn snapshot_bound_admits_exactly_the_bound() {
    let id = ComponentId::new(0xC0A0);
    let registry = registry(&[(id, ComponentType::TlsMapBytesBytes as i32)]);
    let grow = TlsMapDelta::<VLBytes, VLBytes>::new()
        .update(VLBytes::new(vec![8]), VLBytes::new(vec![b'v'; 1305]));
    let proposals = [
        // 56,032 bytes, as in `intermediate_snapshot_bound`.
        (id, insert(&[0, 1, 2, 3, 4, 5, 6].map(|key| (key, 8000)))),
        // 8,196 and 1,308 more bytes: 65,536.
        (id, insert(&[(7, 8192), (8, 1304)])),
        // One byte more.
        (id, update(grow.tls_serialize_detached()?)),
    ];

    let mut set = WorkingSet::new(Cow::Borrowed(&registry), |_| None);
    set.apply(id, &proposals[0].1)?;
    set.apply(id, &proposals[1].1)?;
    assert_eq!(set.snapshot_len(id), Some(MAX_FIELD_SNAPSHOT_BYTES));
    let rejected = set.apply(id, &proposals[2].1);
    assert!(
        matches!(
            rejected,
            Err(ComponentSourceError::FieldBoundExceeded {
                len: 65_537,
                max: MAX_FIELD_SNAPSHOT_BYTES,
                ..
            })
        ),
        "{rejected:?}"
    );
    assert!(matches!(
        reference::apply_sequence(&registry, &BTreeMap::new(), &proposals),
        Err((
            2,
            ComponentSourceError::FieldBoundExceeded { len: 65_537, .. }
        ))
    ));
}

/// One hash index, kept current across proposals, resolves every
/// `RemoveByHash`: 3,000 alternating inserts and hash removals on a
/// 10,000-member admin list hash at most n + m keys.
// verifies: META-013
#[xmtp_common::test(unwrap_try = true)]
fn remove_by_hash_index_incremental() {
    const MEMBERS: usize = 10_000;
    const PROPOSALS: usize = 3_000;
    let inbox = |n: usize| {
        let mut bytes = [0; 32];
        bytes[..8].copy_from_slice(&(n as u64).to_be_bytes());
        InboxId::from_bytes(bytes)
    };
    let committed = TlsSet::from_keys((0..MEMBERS).map(inbox)).tls_serialize_detached()?;
    let registry = ComponentRegistry::new();
    let admins = ComponentId::ADMIN_LIST;
    let mut set = WorkingSet::new(Cow::Borrowed(&registry), |id| {
        (id == admins).then_some(committed.as_slice())
    });

    let before = counters::hashes();
    for n in MEMBERS..MEMBERS + PROPOSALS / 2 {
        let insert = TlsSetDelta::new().insert(inbox(n));
        set.update(admins, &insert.tls_serialize_detached()?)?;
        let remove = TlsSetDelta::<InboxId>::new().remove_by_hash(TlsKeyHash::of(&inbox(n))?);
        set.update(admins, &remove.tls_serialize_detached()?)?;
    }
    let hashed = counters::hashes() - before;
    assert!(hashed <= MEMBERS + PROPOSALS, "{hashed} keys hashed");
    assert_eq!(
        set.encode(admins)?.map(|bytes| bytes.to_vec()),
        Some(committed.clone())
    );
}

/// A `Remove` never decodes the committed bytes, so removing a malformed or
/// unregistered component costs nothing and cannot fail, and an update
/// after it starts from no value.
#[xmtp_common::test(unwrap_try = true)]
fn remove_skips_the_committed_value() {
    let unregistered = ComponentId::new(0xC0A1);
    let admins = ComponentId::ADMIN_LIST;
    let registry = ComponentRegistry::new();
    let malformed = [0xFF; 3];
    let mut set = WorkingSet::new(Cow::Borrowed(&registry), |_| Some(&malformed[..]));

    let before = counters::decodes();
    set.remove(unregistered);
    set.remove(admins);
    let insert = TlsSetDelta::new().insert(InboxId::from_bytes([1; 32]));
    set.update(admins, &insert.tls_serialize_detached()?)?;
    assert_eq!(counters::decodes(), before);
    let states = set.into_states()?;
    let present: Vec<_> = states
        .iter()
        .map(|(id, state)| (*id, state.is_some()))
        .collect();
    assert_eq!(present, [(admins, true), (unregistered, false)]);
}

/// An update that fails without a checkpoint leaves the set as it was: its
/// entries, its length, and which components a proposal wrote.
#[xmtp_common::test(unwrap_try = true)]
fn failed_update_leaves_the_set_unchanged() {
    let alice = InboxId::from_bytes([1; 32]);
    let bob = InboxId::from_bytes([2; 32]);
    let admins = ComponentId::ADMIN_LIST;
    let committed = TlsSet::from_keys([alice]).tls_serialize_detached()?;
    let registry = ComponentRegistry::new();
    let mut set = WorkingSet::new(Cow::Borrowed(&registry), |id| {
        (id == admins).then_some(committed.as_slice())
    });

    // Bob's insert applies before Alice's fails.
    let failing = TlsSetDelta::new().insert(bob).insert(alice);
    let failed = set.update(admins, &failing.tls_serialize_detached()?);
    assert!(
        matches!(
            failed,
            Err(ComponentSourceError::TlsSetApply(TlsSetError::Map(
                TlsMapError::KeyExists
            )))
        ),
        "{failed:?}"
    );
    assert_eq!(
        set.encode(admins)?.map(|bytes| bytes.to_vec()),
        Some(committed.clone())
    );
    assert_eq!(set.snapshot_len(admins), Some(committed.len()));
    assert_eq!(set.touched().count(), 0);

    set.update(admins, &insert_inbox(bob))?;
    assert_eq!(
        set.encode(admins)?.map(|bytes| bytes.to_vec()),
        Some(TlsSet::from_keys([alice, bob]).tls_serialize_detached()?)
    );
}

/// Rolling back an outer checkpoint undoes every proposal since it,
/// including those under a released inner checkpoint and a failed update:
/// each read then matches a set that never saw them.
#[xmtp_common::test(unwrap_try = true)]
fn rollback_undoes_nested_checkpoints() {
    let inbox = |n: u8| InboxId::from_bytes([n; 32]);
    let hash = |n: u8| TlsKeyHash::of(&inbox(n)).unwrap();
    let by_hash = |n: u8| {
        TlsSetDelta::<InboxId>::new()
            .remove_by_hash(hash(n))
            .tls_serialize_detached()
            .unwrap()
    };
    let admins = ComponentId::ADMIN_LIST;
    let members = ComponentId::GROUP_MEMBERSHIP;
    let committed_admins = TlsSet::from_keys([inbox(1), inbox(2)]).tls_serialize_detached()?;
    let committed_members =
        TlsMap::from_pairs([(inbox(1), VLBytes::new(b"a".to_vec()))]).tls_serialize_detached()?;
    let base = |id: ComponentId| match id {
        ComponentId::ADMIN_LIST => Some(committed_admins.as_slice()),
        ComponentId::GROUP_MEMBERSHIP => Some(committed_members.as_slice()),
        _ => None,
    };
    let registry = ComponentRegistry::new();
    let mut set = WorkingSet::new(Cow::Borrowed(&registry), base);
    let mut fresh = WorkingSet::new(Cow::Borrowed(&registry), base);
    set.update(admins, &insert_inbox(inbox(3)))?;
    fresh.update(admins, &insert_inbox(inbox(3)))?;
    // The hash index exists before the checkpoint.
    assert!(set.resolve_hash(admins, &hash(1))?.is_some());

    let outer = set.checkpoint();
    set.update(admins, &by_hash(1))?;
    set.remove(admins);
    set.update(admins, &insert_inbox(inbox(4)))?;
    let inner = set.checkpoint();
    set.update(admins, &by_hash(4))?;
    let entry = TlsMapDelta::new()
        .insert(inbox(9), VLBytes::new(b"x".to_vec()))
        .tls_serialize_detached()?;
    set.update(members, &entry)?;
    set.remove(members);
    set.release(inner);
    assert!(set.update(admins, &by_hash(7)).is_err());
    set.update(members, &entry)?;
    set.rollback(outer);

    for n in 1..=9 {
        assert_eq!(
            set.resolve_hash(admins, &hash(n))?,
            fresh.resolve_hash(admins, &hash(n))?,
            "{n}"
        );
        let key = inbox(n).tls_serialize_detached()?;
        assert_eq!(
            set.get(members, &key)?.map(<[u8]>::to_vec),
            fresh.get(members, &key)?.map(<[u8]>::to_vec),
            "{n}"
        );
    }
    for id in [admins, members] {
        assert_eq!(set.snapshot_len(id), fresh.snapshot_len(id));
        assert_eq!(set.present(id), fresh.present(id));
        assert_eq!(set.encode(id)?, fresh.encode(id)?);
    }
    assert!(set.touched().eq(fresh.touched()));
    assert_eq!(set.into_states()?, fresh.into_states()?);
}

/// Ending another working set's checkpoint is a bug that debug builds
/// catch, instead of undoing the wrong changes.
#[cfg(debug_assertions)]
#[xmtp_common::test(unwrap_try = true)]
#[should_panic(expected = "innermost checkpoint")]
fn ending_another_sets_checkpoint_panics() {
    let registry = ComponentRegistry::new();
    let mut set = WorkingSet::new(Cow::Borrowed(&registry), |_| None);
    let mut other = WorkingSet::new(Cow::Borrowed(&registry), |_| None);
    let checkpoint = set.checkpoint();
    set.update(
        ComponentId::ADMIN_LIST,
        &insert_inbox(InboxId::from_bytes([1; 32])),
    )?;
    set.release(other.checkpoint());
    set.rollback(checkpoint);
}

/// Ending an outer checkpoint while an inner one is live is a bug that
/// debug builds catch.
#[cfg(debug_assertions)]
#[xmtp_common::test(unwrap_try = true)]
#[should_panic(expected = "innermost checkpoint")]
fn ending_an_outer_checkpoint_first_panics() {
    let registry = ComponentRegistry::new();
    let mut set = WorkingSet::new(Cow::Borrowed(&registry), |_| None);
    let outer = set.checkpoint();
    set.update(
        ComponentId::ADMIN_LIST,
        &insert_inbox(InboxId::from_bytes([1; 32])),
    )?;
    let inner = set.checkpoint();
    set.rollback(outer);
    set.rollback(inner);
}

/// A type that does not resolve is not resolved again: the registry entry
/// decodes once per working set, however often an update or a read needs
/// the type.
#[xmtp_common::test(unwrap_try = true)]
fn failed_type_resolution_decodes_the_entry_once() {
    let id = ComponentId::new(0xC0A2);
    let registry = registry(&[(id, 99)]);
    let mut set = WorkingSet::new(Cow::Borrowed(&registry), |_| None);
    let ((), work) = count_validation_work(|| {
        for _ in 0..3 {
            let resolved = set.component_type(id);
            assert!(
                matches!(
                    resolved,
                    Err(ComponentSourceError::MalformedComponentValue { .. })
                ),
                "{resolved:?}"
            );
            assert!(set.update(id, b"x").is_err());
        }
    });
    assert_eq!(work.registry_decodes, 1);
}

/// A registry read through the working set hides an entry the registry
/// does not recognize, as `ComponentRegistry::get` does, and sees the
/// proposals applied so far.
#[xmtp_common::test(unwrap_try = true)]
fn registry_entry_hides_unrecognized_entries() {
    let app = ComponentId::new(0xC0A3);
    let entry = |ty: ComponentType| VLBytes::new(metadata(ty as i32).encode_to_vec());
    // An admin-list entry violates the admin list's policy allowlist.
    let committed = TlsMap::from_pairs([
        (ComponentId::ADMIN_LIST, entry(ComponentType::TlsSetInboxId)),
        (app, entry(ComponentType::Bytes)),
    ])
    .tls_serialize_detached()?;
    let parsed = ComponentRegistry::from_bytes(&committed)?;
    let registry = ComponentRegistry::new();
    let mut set = WorkingSet::new(Cow::Borrowed(&registry), |id| {
        (id == ComponentId::COMPONENT_REGISTRY).then_some(committed.as_slice())
    });

    let admin_key = ComponentId::ADMIN_LIST.tls_serialize_detached()?;
    assert!(
        set.get(ComponentId::COMPONENT_REGISTRY, &admin_key)?
            .is_some()
    );
    for id in [ComponentId::ADMIN_LIST, app, ComponentId::new(0xC0A4)] {
        assert_eq!(set.registry_entry(id)?, parsed.get(&id)?, "{id}");
    }
    assert!(set.registry_entry(ComponentId::ADMIN_LIST)?.is_none());

    let retype = TlsMapDelta::new().update(app, entry(ComponentType::String));
    set.update(
        ComponentId::COMPONENT_REGISTRY,
        &retype.tls_serialize_detached()?,
    )?;
    assert_eq!(
        set.registry_entry(app)?.map(|meta| meta.component_type),
        Some(ComponentType::String as i32)
    );
}

/// `resolve_hash` resolves the component's type first, as the expansion of
/// a `RemoveByHash` does: an id with no built-in impl and no registry entry
/// fails even when it is absent, as `get` does.
#[xmtp_common::test(unwrap_try = true)]
fn resolve_hash_of_an_unknown_component_fails_as_the_expansion_does() {
    let id = ComponentId::new(0xC0B0);
    let alice = InboxId::from_bytes([1; 32]);
    let hash = TlsKeyHash::of(&alice)?;
    let registry = ComponentRegistry::new();
    let mut set = WorkingSet::new(Cow::Borrowed(&registry), |_| None);
    let operation = update(
        TlsSetDelta::<InboxId>::new()
            .remove_by_hash(hash)
            .tls_serialize_detached()?,
    );

    let expanded = expand_app_data_update_to_changes(id, &operation, None, &registry);
    assert!(
        matches!(expanded, Err(ComponentSourceError::UnknownComponent(_))),
        "{expanded:?}"
    );
    let resolved = set.resolve_hash(id, &hash);
    assert!(
        matches!(resolved, Err(ComponentSourceError::UnknownComponent(_))),
        "{resolved:?}"
    );
    let read = set
        .get(id, &alice.tls_serialize_detached()?)
        .map(|value| value.map(<[u8]>::to_vec));
    assert!(
        matches!(read, Err(ComponentSourceError::UnknownComponent(_))),
        "{read:?}"
    );
}

/// A collection that a read decoded, or that a rolled-back proposal wrote,
/// still has its committed bytes, as one never read does. Only a kept
/// write makes them unreadable without an encode.
#[xmtp_common::test(unwrap_try = true)]
fn bytes_of_a_read_collection_are_the_committed_bytes() {
    let super_admins = ComponentId::SUPER_ADMIN_LIST;
    let committed = TlsSet::from_keys([InboxId::from_bytes([1; 32])]).tls_serialize_detached()?;
    let registry = ComponentRegistry::new();
    let mut set = WorkingSet::new(Cow::Borrowed(&registry), |id| {
        (id == super_admins).then_some(committed.as_slice())
    });

    assert_eq!(set.bytes(super_admins)?, Some(committed.as_slice()));
    assert_eq!(set.is_empty(super_admins)?, Some(false));
    assert_eq!(set.bytes(super_admins)?, Some(committed.as_slice()));
    let insert = TlsSetDelta::new()
        .insert(InboxId::from_bytes([2; 32]))
        .tls_serialize_detached()?;
    let checkpoint = set.checkpoint();
    set.update(super_admins, &insert)?;
    set.rollback(checkpoint);
    assert_eq!(set.bytes(super_admins)?, Some(committed.as_slice()));
    set.remove(super_admins);
    assert_eq!(set.is_empty(super_admins)?, None);
    assert_eq!(set.bytes(super_admins)?, None);
    set.update(super_admins, &insert)?;
    assert!(matches!(
        set.bytes(super_admins),
        Err(ComponentSourceError::MismatchedMutation(id)) if id == super_admins
    ));
}

/// Each record and undo step a working set moves to the heap is a few words
/// long. Moving an enum copies its largest variant's size, so a short
/// variant of a large enum carries the unwritten stack bytes under its tail,
/// which can hold a key, into a block that is later freed unwiped.
#[xmtp_common::test(unwrap_try = true)]
fn heap_records_have_no_long_unwritten_tail() {
    use std::mem::size_of;

    use super::{Change, Tracked, Undo};

    let word = size_of::<usize>();
    assert!(
        size_of::<Tracked<'_>>() <= 4 * word,
        "{}",
        size_of::<Tracked<'_>>()
    );
    assert!(
        size_of::<Undo<'_>>() <= 2 * word,
        "{}",
        size_of::<Undo<'_>>()
    );
    assert!(size_of::<Change>() <= 6 * word, "{}", size_of::<Change>());
}
