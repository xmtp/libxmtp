use std::{borrow::Cow, collections::BTreeMap};

use openmls::messages::proposals::AppDataUpdateOperation;
use tls_codec::{Serialize, VLBytes};
use xmtp_mls_tls_collections::{
    tls_map::TlsMapDelta,
    tls_set::{TlsKeyHash, TlsSet, TlsSetDelta},
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
        component_source::{ComponentSourceError, MAX_FIELD_SNAPSHOT_BYTES},
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

/// A proposal that leaves a bounded snapshot at 65,537 bytes fails, even
/// when a later proposal of the commit shrinks it below the bound.
// verifies: META-068
#[xmtp_common::test(unwrap_try = true)]
fn intermediate_snapshot_bound() {
    let id = ComponentId::new(0xC0A0);
    let registry = registry(&[(id, ComponentType::TlsMapBytesBytes as i32)]);
    let insert = |entries: &[(u8, usize)]| {
        let delta = entries
            .iter()
            .fold(TlsMapDelta::new(), |delta, &(key, len)| {
                delta.insert(VLBytes::new(vec![key]), VLBytes::new(vec![b'v'; len]))
            });
        update(delta.tls_serialize_detached().unwrap())
    };
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

/// Reads see the proposals applied so far, and a rollback undoes them.
#[xmtp_common::test(unwrap_try = true)]
fn reads_follow_applied_proposals() {
    let alice = InboxId::from_bytes([1; 32]);
    let bob = InboxId::from_bytes([2; 32]);
    let admins = ComponentId::ADMIN_LIST;
    let members = ComponentId::GROUP_MEMBERSHIP;
    let committed = TlsSet::from_keys([alice]).tls_serialize_detached()?;
    let registry = ComponentRegistry::new();
    let mut set = WorkingSet::new(Cow::Borrowed(&registry), |id| {
        (id == admins).then_some(committed.as_slice())
    });
    let bob_hash = TlsKeyHash::of(&bob)?;

    assert_eq!(
        set.resolve_hash(admins, &TlsKeyHash::of(&alice)?)?,
        Some(alice.as_bytes().to_vec())
    );
    assert_eq!(set.resolve_hash(admins, &bob_hash)?, None);
    let checkpoint = set.checkpoint();
    set.update(
        admins,
        &TlsSetDelta::new().insert(bob).tls_serialize_detached()?,
    )?;
    assert_eq!(
        set.resolve_hash(admins, &bob_hash)?,
        Some(bob.as_bytes().to_vec())
    );
    assert_eq!(
        set.get(admins, &bob.tls_serialize_detached()?)?,
        Some(&[][..])
    );
    let entry = TlsMapDelta::new().insert(bob, VLBytes::new(b"entry".to_vec()));
    set.update(members, &entry.tls_serialize_detached()?)?;
    assert_eq!(
        set.get(members, &bob.tls_serialize_detached()?)?,
        Some(&b"entry"[..])
    );
    set.rollback(checkpoint);

    assert_eq!(set.resolve_hash(admins, &bob_hash)?, None);
    assert_eq!(set.get(members, &bob.tls_serialize_detached()?)?, None);
    assert!(!set.present(members));
    assert_eq!(set.touched().count(), 0);
}
