//! A [`WorkingSet`] against the frozen byte-level apply in
//! [`super::reference`], on random committed values and proposal
//! sequences: the same verdict, error class, lengths, and final bytes
//! after every proposal, in a receiver's commit and in a sender's
//! keep-or-roll-back loop.

use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
    sync::OnceLock,
};

use openmls::messages::proposals::AppDataUpdateOperation;
use proptest::{
    collection::vec,
    prelude::*,
    test_runner::{Config, TestCaseError, TestRunner},
};
use prost::Message;
use tls_codec::{Serialize, Size, VLBytes};
use xmtp_cryptography::secret::Zeroizing;
use xmtp_mls_tls_collections::{
    tls_map::{TlsMap, TlsMapDelta, TlsMapEntry, TlsMapError},
    tls_set::{TlsKeyHash, TlsSetDelta, TlsSetError},
};
use xmtp_proto::xmtp::mls::message_contents::ComponentType;

use super::{WorkingSet, metadata, reference, registry, update};
use crate::{
    app_data::{component_id::ComponentId, component_source::ComponentSourceError},
    inbox_id::InboxId,
};

const APP_MAP: ComponentId = ComponentId::new(0xC001);
const APP_SET: ComponentId = ComponentId::new(0xC002);
const APP_NAMES: ComponentId = ComponentId::new(0xC003);
const APP_INBOX_SET: ComponentId = ComponentId::new(0xC004);
const APP_INBOX_MAP: ComponentId = ComponentId::new(0xC005);
const APP_STRING: ComponentId = ComponentId::new(0xC006);
const APP_UNSPECIFIED: ComponentId = ComponentId::new(0xC007);
const APP_BAD_TAG: ComponentId = ComponentId::new(0xC008);
const APP_UNREGISTERED: ComponentId = ComponentId::new(0xC009);
const APP_IMMUTABLE: ComponentId = ComponentId::new(0xFD01);
const XMTP_MAP: ComponentId = ComponentId::new(0x8100);
const XMTP_SET: ComponentId = ComponentId::new(0x8101);
const XMTP_NAMES: ComponentId = ComponentId::new(0x8102);
const XMTP_INBOX_MAP: ComponentId = ComponentId::new(0x8103);
const XMTP_IMMUTABLE: ComponentId = ComponentId::new(0xBE01);

/// How the generator writes a component.
#[derive(Clone, Copy, Debug)]
enum Kind {
    BytesMap,
    BytesSet,
    InboxMap,
    InboxSet,
    Registry,
    Scalar,
}

/// The components under test: every collection shape, bounded and not,
/// built-in and registry-typed, immutable, unregistered, and badly typed.
const SUBJECTS: [(ComponentId, Kind); 28] = [
    (APP_MAP, Kind::BytesMap),
    (APP_SET, Kind::BytesSet),
    (APP_NAMES, Kind::InboxMap),
    (APP_INBOX_SET, Kind::InboxSet),
    (APP_INBOX_MAP, Kind::InboxMap),
    (APP_STRING, Kind::Scalar),
    (APP_UNSPECIFIED, Kind::BytesMap),
    (APP_BAD_TAG, Kind::BytesMap),
    (APP_UNREGISTERED, Kind::BytesMap),
    (APP_IMMUTABLE, Kind::BytesMap),
    (XMTP_MAP, Kind::BytesMap),
    (XMTP_SET, Kind::BytesSet),
    (XMTP_NAMES, Kind::InboxMap),
    (XMTP_INBOX_MAP, Kind::InboxMap),
    (XMTP_IMMUTABLE, Kind::BytesMap),
    (ComponentId::ADMIN_LIST, Kind::InboxSet),
    (ComponentId::SUPER_ADMIN_LIST, Kind::InboxSet),
    (ComponentId::DM_MEMBERS, Kind::InboxSet),
    (ComponentId::GROUP_MEMBERSHIP, Kind::InboxMap),
    (ComponentId::USER_DISPLAY_NAME, Kind::InboxMap),
    (ComponentId::COMPONENT_REGISTRY, Kind::Registry),
    (ComponentId::GROUP_NAME, Kind::Scalar),
    (ComponentId::GROUP_IMAGE, Kind::Scalar),
    (ComponentId::GROUP_ROOT_KEY, Kind::Scalar),
    (ComponentId::MESSAGE_DISAPPEAR_IN_NS, Kind::Scalar),
    (ComponentId::COMMIT_LOG_SIGNER, Kind::Scalar),
    (ComponentId::CONVERSATION_TYPE, Kind::Scalar),
    (ComponentId::CREATOR_INBOX_ID, Kind::Scalar),
];

/// The registry the subjects resolve against.
fn subject_registry() -> crate::app_data::component_registry::ComponentRegistry {
    let tag = |ty: ComponentType| ty as i32;
    registry(&[
        (APP_MAP, tag(ComponentType::TlsMapBytesBytes)),
        (APP_SET, tag(ComponentType::TlsSetBytes)),
        (APP_NAMES, tag(ComponentType::TlsMapInboxIdString)),
        (APP_INBOX_SET, tag(ComponentType::TlsSetInboxId)),
        (APP_INBOX_MAP, tag(ComponentType::TlsMapInboxIdBytes)),
        (APP_STRING, tag(ComponentType::String)),
        (APP_UNSPECIFIED, tag(ComponentType::Unspecified)),
        (APP_BAD_TAG, 99),
        (APP_IMMUTABLE, tag(ComponentType::TlsMapBytesBytes)),
        (XMTP_MAP, tag(ComponentType::TlsMapBytesBytes)),
        (XMTP_SET, tag(ComponentType::TlsSetBytes)),
        (XMTP_NAMES, tag(ComponentType::TlsMapInboxIdString)),
        (XMTP_INBOX_MAP, tag(ComponentType::TlsMapInboxIdBytes)),
    ])
}

/// The elements the generator draws from. Byte keys `0..12` are distinct
/// one-byte keys; the rest, like the values, sit on the length-prefix
/// boundaries (63/64, 16383/16384) and around the element bound.
struct Pools {
    keys: Vec<Vec<u8>>,
    values: Vec<Vec<u8>>,
    scalars: Vec<Vec<u8>>,
    registry_values: Vec<Vec<u8>>,
}

/// Distinct inbox ids.
const INBOXES: usize = 12;
/// Registry keys: writable, immutable, reserved, hardcoded, well-known, and
/// ids below the component space whose varints are 1, 2, and 4 bytes long,
/// which only a committed registry can hold.
const REGISTRY_KEYS: [u16; 10] = [
    0xC010, 0xC011, 0xFD10, 0xFF00, 0x8000, 0x8004, 0x003F, 0x0040, 0x3FFF, 0x4000,
];
/// The value index of an 8000-byte value.
const HEAVY_VALUE: u8 = 6;

fn pools() -> &'static Pools {
    static POOLS: OnceLock<Pools> = OnceLock::new();
    POOLS.get_or_init(|| {
        let mut keys: Vec<Vec<u8>> = (0..12).map(|i| vec![b'a' + i]).collect();
        keys.extend([
            vec![],
            vec![0xFF],
            vec![b'k'; 63],
            vec![b'k'; 64],
            vec![b'x'; 8192],
            vec![b'y'; 8193],
            vec![b'z'; 16383],
            vec![b'w'; 16384],
        ]);
        Pools {
            keys,
            values: vec![
                vec![],
                b"v".to_vec(),
                vec![0xFF],
                "\u{e9}".as_bytes().to_vec(),
                vec![b'v'; 63],
                vec![b'v'; 64],
                vec![b'V'; 8000],
                vec![b'V'; 8192],
                vec![b'W'; 8193],
                vec![b'u'; 16383],
                vec![b'u'; 16384],
            ],
            scalars: vec![
                vec![],
                b"name".to_vec(),
                vec![0xFF],
                7_i64.to_be_bytes().to_vec(),
                vec![7; 32],
                vec![7; 31],
                vec![b'a'; 8192],
                vec![b'a'; 8193],
            ],
            registry_values: vec![
                metadata(ComponentType::Bytes as i32).encode_to_vec(),
                metadata(ComponentType::TlsMapBytesBytes as i32).encode_to_vec(),
                vec![0xFF, 0xFF],
                vec![],
            ],
        }
    })
}

fn pick(pool: &[Vec<u8>], index: u8) -> &[u8] {
    &pool[index as usize % pool.len()]
}

fn vl(bytes: &[u8]) -> VLBytes {
    VLBytes::new(bytes.to_vec())
}

fn inbox(index: u8) -> InboxId {
    InboxId::from_bytes([(index as usize % INBOXES) as u8; 32])
}

fn registry_key(index: u8) -> ComponentId {
    ComponentId::new(REGISTRY_KEYS[index as usize % REGISTRY_KEYS.len()])
}

#[derive(Clone, Copy, Debug)]
enum MutationSpec {
    Insert(u8, u8),
    Update(u8, u8),
    Delete(u8),
    ByHash(u8),
}

#[derive(Clone, Debug)]
enum OperationSpec {
    /// A delta, and whether to turn each mutation the generator expects to
    /// fail into one it expects to apply.
    Delta(Vec<MutationSpec>, bool),
    Remove,
    Scalar(u8),
    Raw(Vec<u8>),
}

#[derive(Clone, Debug)]
enum Corruption {
    Truncate(u16),
    Trailing(u8),
    Reversed,
    NonMinimalLength,
    Flip(u16, u8),
}

#[derive(Clone, Debug)]
enum BaseSpec {
    Absent,
    Entries(Vec<(u8, u8)>),
    /// This many entries of 8000 bytes, around the snapshot bound.
    Heavy(u8),
    Corrupt(Vec<(u8, u8)>, Corruption),
    Raw(Vec<u8>),
}

#[derive(Clone, Debug)]
struct Case {
    /// Each component the case writes, as an index of [`SUBJECTS`], and its
    /// committed value.
    subjects: Vec<(u8, BaseSpec)>,
    /// Each proposal: which of `subjects` it writes, its operation, and
    /// whether a sender drops it after it applies.
    proposals: Vec<(u8, OperationSpec, bool)>,
}

type Proposals = Vec<(ComponentId, AppDataUpdateOperation, bool)>;

impl Case {
    fn build(&self) -> (BTreeMap<ComponentId, Vec<u8>>, Proposals) {
        let subjects: Vec<_> = self
            .subjects
            .iter()
            .map(|(index, _)| SUBJECTS[*index as usize % SUBJECTS.len()])
            .collect();
        let mut base = BTreeMap::new();
        // The keys the generator expects each subject to hold.
        let mut held = Vec::new();
        for ((id, kind), (_, spec)) in subjects.iter().zip(&self.subjects) {
            if let Some(bytes) = committed(*kind, spec) {
                base.insert(*id, bytes);
            }
            held.push(match spec {
                BaseSpec::Entries(entries) => {
                    entries.iter().map(|(k, _)| normal(*kind, *k)).collect()
                }
                BaseSpec::Heavy(count) => (0..*count as usize).collect(),
                _ => BTreeSet::new(),
            });
        }
        let proposals = self
            .proposals
            .iter()
            .map(|(slot, spec, dropped)| {
                let slot = *slot as usize % subjects.len();
                let (id, kind) = subjects[slot];
                let operation = match spec {
                    OperationSpec::Delta(mutations, repair) => {
                        let mutations = expect(kind, mutations, *repair, &mut held[slot]);
                        update(delta(kind, &mutations))
                    }
                    OperationSpec::Remove => {
                        held[slot].clear();
                        AppDataUpdateOperation::Remove
                    }
                    OperationSpec::Scalar(index) => update(pick(&pools().scalars, *index).to_vec()),
                    OperationSpec::Raw(bytes) => update(bytes.clone()),
                };
                (id, operation, *dropped)
            })
            .collect();
        (base, proposals)
    }
}

/// The key index `index` names in a component of `kind`.
fn normal(kind: Kind, index: u8) -> usize {
    let keys = match kind {
        Kind::InboxMap | Kind::InboxSet => INBOXES,
        Kind::Registry => REGISTRY_KEYS.len(),
        Kind::BytesMap | Kind::BytesSet | Kind::Scalar => pools().keys.len(),
    };
    index as usize % keys
}

/// `mutations`, each turned into one that applies to the keys `held` when
/// `repair`, and `held` after them. A set reads an update as a removal and a
/// map reads a hash removal as a deletion.
fn expect(
    kind: Kind,
    mutations: &[MutationSpec],
    repair: bool,
    held: &mut BTreeSet<usize>,
) -> Vec<MutationSpec> {
    let set = matches!(kind, Kind::BytesSet | Kind::InboxSet);
    mutations
        .iter()
        .map(|mutation| {
            let key = |k| normal(kind, k);
            let mutation = match *mutation {
                MutationSpec::Insert(k, v) if repair && held.contains(&key(k)) => {
                    if set {
                        MutationSpec::Delete(k)
                    } else {
                        MutationSpec::Update(k, v)
                    }
                }
                MutationSpec::Update(k, v) if repair && !held.contains(&key(k)) => {
                    MutationSpec::Insert(k, v)
                }
                MutationSpec::Delete(k) | MutationSpec::ByHash(k)
                    if repair && !held.contains(&key(k)) =>
                {
                    MutationSpec::Insert(k, 1)
                }
                other => other,
            };
            match mutation {
                MutationSpec::Insert(k, _) => {
                    held.insert(key(k));
                }
                MutationSpec::Update(k, _) if set => {
                    held.remove(&key(k));
                }
                MutationSpec::Update(..) => {}
                MutationSpec::Delete(k) | MutationSpec::ByHash(k) => {
                    held.remove(&key(k));
                }
            }
            mutation
        })
        .collect()
}

fn committed(kind: Kind, spec: &BaseSpec) -> Option<Vec<u8>> {
    Some(match spec {
        BaseSpec::Absent => return None,
        BaseSpec::Entries(entries) => encode_entries(kind, entries, false),
        BaseSpec::Heavy(count) => heavy(kind, *count),
        BaseSpec::Corrupt(entries, corruption) => corrupt(kind, entries, corruption),
        BaseSpec::Raw(bytes) => bytes.clone(),
    })
}

/// `count` entries of 8000 bytes each: a set's keys, or a map's values.
fn heavy(kind: Kind, count: u8) -> Vec<u8> {
    match kind {
        Kind::BytesSet => encode_map((0..count).map(|i| (vl(&[b'a' + i; 8000]), ())), false),
        _ => encode_entries(
            kind,
            &(0..count).map(|i| (i, HEAVY_VALUE)).collect::<Vec<_>>(),
            false,
        ),
    }
}

/// A snapshot of `entries`, in key order or, when `reversed`, the reverse.
fn encode_entries(kind: Kind, entries: &[(u8, u8)], reversed: bool) -> Vec<u8> {
    let pools = pools();
    let key = |k| vl(pick(&pools.keys, k));
    let value = |v| vl(pick(&pools.values, v));
    match kind {
        Kind::BytesMap => encode_map(entries.iter().map(|&(k, v)| (key(k), value(v))), reversed),
        Kind::BytesSet => encode_map(entries.iter().map(|&(k, _)| (key(k), ())), reversed),
        Kind::InboxMap => encode_map(entries.iter().map(|&(k, v)| (inbox(k), value(v))), reversed),
        Kind::InboxSet => encode_map(entries.iter().map(|&(k, _)| (inbox(k), ())), reversed),
        Kind::Registry => encode_map(
            entries
                .iter()
                .map(|&(k, v)| (registry_key(k), vl(pick(&pools.registry_values, v)))),
            reversed,
        ),
        Kind::Scalar => entries
            .first()
            .map_or_else(Vec::new, |&(_, v)| pick(&pools.scalars, v).to_vec()),
    }
}

fn encode_map<K, V>(pairs: impl Iterator<Item = (K, V)>, reversed: bool) -> Vec<u8>
where
    K: Ord + Serialize + Size + std::fmt::Debug,
    V: Serialize + Size + std::fmt::Debug,
{
    let map = TlsMap::from_pairs(pairs);
    if !reversed {
        return map.tls_serialize_detached().unwrap();
    }
    let entries: Vec<_> = map
        .into_iter()
        .rev()
        .map(|(key, value)| TlsMapEntry { key, value })
        .collect();
    entries.tls_serialize_detached().unwrap()
}

fn corrupt(kind: Kind, entries: &[(u8, u8)], corruption: &Corruption) -> Vec<u8> {
    let mut bytes = encode_entries(kind, entries, matches!(corruption, Corruption::Reversed));
    match corruption {
        Corruption::Truncate(len) if !bytes.is_empty() => {
            bytes.truncate(*len as usize % bytes.len());
        }
        Corruption::Trailing(byte) => bytes.push(*byte),
        Corruption::NonMinimalLength => {
            let mut rest = bytes.as_slice();
            if let Ok((len, prefix)) = tls_codec::vlen::read_length(&mut rest) {
                // The same length in the next wider form.
                let mut wider = if len <= 0x3fff {
                    (0x8000_0000_u32 | len as u32).to_be_bytes().to_vec()
                } else {
                    (0xC000_0000_0000_0000_u64 | len as u64)
                        .to_be_bytes()
                        .to_vec()
                };
                wider.extend_from_slice(&bytes[prefix..]);
                bytes = wider;
            }
        }
        Corruption::Flip(at, mask) if !bytes.is_empty() => {
            let at = *at as usize % bytes.len();
            bytes[at] ^= *mask;
        }
        _ => {}
    }
    bytes
}

/// The delta payload of `mutations` for a component of `kind`. A map turns a
/// hash removal into a deletion; a set turns an update into a removal.
fn delta(kind: Kind, mutations: &[MutationSpec]) -> Vec<u8> {
    let pools = pools();
    let key = |k| vl(pick(&pools.keys, k));
    let value = |v| vl(pick(&pools.values, v));
    match kind {
        Kind::BytesMap | Kind::Scalar => map_delta(mutations, key, value),
        Kind::InboxMap => map_delta(mutations, inbox, value),
        Kind::Registry => map_delta(mutations, registry_key, |v| {
            vl(pick(&pools.registry_values, v))
        }),
        Kind::BytesSet => set_delta(mutations, key),
        Kind::InboxSet => set_delta(mutations, inbox),
    }
}

fn map_delta<K: Serialize + Size + std::fmt::Debug>(
    mutations: &[MutationSpec],
    key: impl Fn(u8) -> K,
    value: impl Fn(u8) -> VLBytes,
) -> Vec<u8> {
    let delta = mutations
        .iter()
        .fold(TlsMapDelta::new(), |delta, mutation| match *mutation {
            MutationSpec::Insert(k, v) => delta.insert(key(k), value(v)),
            MutationSpec::Update(k, v) => delta.update(key(k), value(v)),
            MutationSpec::Delete(k) | MutationSpec::ByHash(k) => delta.delete(key(k)),
        });
    delta.tls_serialize_detached().unwrap()
}

fn set_delta<K: Serialize + Size + std::fmt::Debug>(
    mutations: &[MutationSpec],
    key: impl Fn(u8) -> K,
) -> Vec<u8> {
    let delta = mutations
        .iter()
        .fold(TlsSetDelta::new(), |delta, mutation| match *mutation {
            MutationSpec::Insert(k, _) => delta.insert(key(k)),
            MutationSpec::Update(k, _) | MutationSpec::Delete(k) => delta.remove(key(k)),
            MutationSpec::ByHash(k) => delta.remove_by_hash(TlsKeyHash::of(&key(k)).unwrap()),
        });
    delta.tls_serialize_detached().unwrap()
}

/// What members must agree on about an error: its variant, the apply
/// error's kind, a field bound's numbers, and the ids, but no message.
fn class(error: &ComponentSourceError) -> String {
    let map = |error: &TlsMapError| match error {
        TlsMapError::KeyExists => "KeyExists",
        TlsMapError::KeyNotFound => "KeyNotFound",
        TlsMapError::Codec(_) => "Codec",
    };
    match error {
        ComponentSourceError::MalformedComponentValue { component_id, .. } => {
            format!("Malformed({component_id})")
        }
        ComponentSourceError::TlsCodec(_) => "TlsCodec".to_owned(),
        ComponentSourceError::TlsSetApply(TlsSetError::Map(error)) => {
            format!("TlsSetApply({})", map(error))
        }
        ComponentSourceError::TlsSetApply(TlsSetError::DuplicateHash) => {
            "TlsSetApply(DuplicateHash)".to_owned()
        }
        ComponentSourceError::TlsSetApply(TlsSetError::Codec(_)) => "TlsSetApply(Codec)".to_owned(),
        ComponentSourceError::TlsMapApply(error) => format!("TlsMapApply({})", map(error)),
        other => format!("{other:?}"),
    }
}

type Plain = BTreeMap<ComponentId, Option<Vec<u8>>>;

fn plain(states: impl IntoIterator<Item = (ComponentId, Option<Zeroizing<Vec<u8>>>)>) -> Plain {
    states
        .into_iter()
        .map(|(id, state)| (id, state.map(|bytes| bytes.to_vec())))
        .collect()
}

fn len(bytes: Option<&[u8]>) -> Option<usize> {
    bytes.map(<[u8]>::len)
}

/// A receiver's commit: every proposal applies in order, and the first
/// failure rejects the commit.
fn check_commit(
    registry: &crate::app_data::component_registry::ComponentRegistry,
    base: &BTreeMap<ComponentId, Vec<u8>>,
    proposals: &Proposals,
) -> Result<(), TestCaseError> {
    let mut set = WorkingSet::new(Cow::Borrowed(registry), |id| {
        base.get(&id).map(Vec::as_slice)
    });
    let mut expected = reference::States::new();
    for (index, (id, operation, _)) in proposals.iter().enumerate() {
        let want = reference::apply_one(registry, base, &mut expected, *id, operation)
            .map_err(|e| class(&e));
        let got = set.apply(*id, operation).map_err(|e| class(&e));
        prop_assert_eq!(&got, &want, "proposal {} to {}", index, id);
        if want.is_err() {
            return Ok(());
        }
        let bytes = reference::current(&expected, base, *id);
        prop_assert_eq!(
            set.snapshot_len(*id),
            len(bytes),
            "length after proposal {}",
            index
        );
        prop_assert_eq!(set.present(*id), bytes.is_some());
    }
    let got = set
        .into_states()
        .map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
    prop_assert_eq!(plain(got), plain(expected));
    Ok(())
}

/// A sender's loop: each proposal is kept when it applies after the ones
/// kept before it and the sender does not drop it, and rolled back
/// otherwise.
fn check_keep(
    registry: &crate::app_data::component_registry::ComponentRegistry,
    base: &BTreeMap<ComponentId, Vec<u8>>,
    proposals: &Proposals,
) -> Result<(), TestCaseError> {
    let mut set = WorkingSet::new(Cow::Borrowed(registry), |id| {
        base.get(&id).map(Vec::as_slice)
    });
    let mut kept = reference::States::new();
    for (index, (id, operation, dropped)) in proposals.iter().enumerate() {
        let mut trial = kept.clone();
        let want =
            reference::apply_one(registry, base, &mut trial, *id, operation).map_err(|e| class(&e));
        let checkpoint = set.checkpoint();
        let got = set.apply(*id, operation).map_err(|e| class(&e));
        prop_assert_eq!(&got, &want, "proposal {} to {}", index, id);
        if want.is_ok() && !dropped {
            kept = trial;
            set.release(checkpoint);
        } else {
            set.rollback(checkpoint);
        }
        for (id, _, _) in proposals {
            let bytes = reference::current(&kept, base, *id);
            prop_assert_eq!(
                set.snapshot_len(*id),
                len(bytes),
                "length after proposal {}",
                index
            );
            prop_assert_eq!(set.present(*id), bytes.is_some());
        }
    }
    let got = set
        .into_states()
        .map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
    prop_assert_eq!(plain(got), plain(kept));
    Ok(())
}

fn check(case: &Case) -> Result<(), TestCaseError> {
    let registry = subject_registry();
    let (base, proposals) = case.build();
    check_commit(&registry, &base, &proposals)?;
    check_keep(&registry, &base, &proposals)
}

/// Mostly the distinct one-byte keys, so mutations meet each other.
fn key_index() -> impl Strategy<Value = u8> {
    prop_oneof![20 => 0_u8..12, 2 => 12_u8..16, 1 => 16_u8..20]
}

/// Mostly values that fit the element bound, a fifth near it.
fn value_index() -> impl Strategy<Value = u8> {
    prop_oneof![10 => 0_u8..4, 3 => 4_u8..6, 4 => 6_u8..8, 1 => 8_u8..11]
}

fn mutation() -> impl Strategy<Value = MutationSpec> {
    prop_oneof![
        (key_index(), value_index()).prop_map(|(k, v)| MutationSpec::Insert(k, v)),
        (key_index(), value_index()).prop_map(|(k, v)| MutationSpec::Update(k, v)),
        key_index().prop_map(MutationSpec::Delete),
        key_index().prop_map(MutationSpec::ByHash),
    ]
}

fn operation_spec() -> impl Strategy<Value = OperationSpec> {
    prop_oneof![
        6 => (vec(mutation(), 0..6), prop::bool::weighted(0.75))
            .prop_map(|(mutations, repair)| OperationSpec::Delta(mutations, repair)),
        2 => Just(OperationSpec::Remove),
        1 => (0_u8..8).prop_map(OperationSpec::Scalar),
        1 => vec(any::<u8>(), 0..8).prop_map(OperationSpec::Raw),
    ]
}

fn corruption() -> impl Strategy<Value = Corruption> {
    prop_oneof![
        any::<u16>().prop_map(Corruption::Truncate),
        any::<u8>().prop_map(Corruption::Trailing),
        Just(Corruption::Reversed),
        Just(Corruption::NonMinimalLength),
        (any::<u16>(), 1_u8..).prop_map(|(at, mask)| Corruption::Flip(at, mask)),
    ]
}

fn base_spec() -> impl Strategy<Value = BaseSpec> {
    let entries = || vec((key_index(), value_index()), 0..8);
    prop_oneof![
        2 => Just(BaseSpec::Absent),
        5 => entries().prop_map(BaseSpec::Entries),
        1 => (6_u8..11).prop_map(BaseSpec::Heavy),
        2 => (entries(), corruption()).prop_map(|(e, c)| BaseSpec::Corrupt(e, c)),
        1 => vec(any::<u8>(), 0..12).prop_map(BaseSpec::Raw),
    ]
}

/// Mostly the collections a delta can apply to.
fn subject_index() -> impl Strategy<Value = u8> {
    let collections = [
        APP_MAP,
        APP_SET,
        APP_NAMES,
        APP_INBOX_SET,
        APP_INBOX_MAP,
        XMTP_MAP,
        XMTP_SET,
        XMTP_NAMES,
        XMTP_INBOX_MAP,
        ComponentId::ADMIN_LIST,
        ComponentId::GROUP_MEMBERSHIP,
        ComponentId::USER_DISPLAY_NAME,
        ComponentId::COMPONENT_REGISTRY,
    ];
    prop_oneof![
        3 => prop::sample::select(collections.map(subject).to_vec()),
        2 => 0..SUBJECTS.len() as u8,
    ]
}

fn case() -> impl Strategy<Value = Case> {
    let subjects = vec((subject_index(), base_spec()), 1..4);
    let proposals = vec(
        (any::<u8>(), operation_spec(), prop::bool::weighted(0.25)),
        1..12,
    );
    (subjects, proposals).prop_map(|(subjects, proposals)| Case {
        subjects,
        proposals,
    })
}

/// A working set reaches the byte-level apply's verdict, error class,
/// lengths, and bytes on random committed values and proposals.
// verifies: META-010, META-012, META-013, META-068
#[xmtp_common::test]
fn working_set_matches_byte_level_apply() {
    let mut runner = TestRunner::new(Config {
        cases: 1024,
        ..Config::default()
    });
    runner.run(&case(), |case| check(&case)).unwrap();
}

fn subject(id: ComponentId) -> u8 {
    SUBJECTS
        .iter()
        .position(|(subject, _)| *subject == id)
        .unwrap() as u8
}

/// The cases the plan names, on every run: a hash removal of a key the
/// same delta inserts, a removal then an update, the length-prefix
/// boundaries, and malformed committed bytes.
// verifies: META-012, META-013
#[xmtp_common::test]
fn working_set_matches_byte_level_apply_on_pinned_cases() {
    use BaseSpec::{Absent, Corrupt, Entries, Heavy};
    use MutationSpec::{ByHash, Delete, Insert, Update};
    use OperationSpec::Remove;
    // The mutations as written, not repaired.
    let delta = |mutations| OperationSpec::Delta(mutations, false);
    let one = |id: ComponentId, base: BaseSpec, proposals: Vec<OperationSpec>| Case {
        subjects: vec![(subject(id), base)],
        proposals: proposals.into_iter().map(|p| (0, p, false)).collect(),
    };
    let cases = [
        // `[Insert(k), RemoveByHash(h(k))]` with `k` absent: KeyNotFound.
        one(
            ComponentId::ADMIN_LIST,
            Absent,
            vec![delta(vec![Insert(1, 0), ByHash(1)])],
        ),
        one(
            APP_SET,
            Entries(vec![(2, 0)]),
            vec![delta(vec![Insert(1, 0), ByHash(1), ByHash(2)])],
        ),
        // Hashes resolve against the set before the delta.
        one(
            ComponentId::ADMIN_LIST,
            Entries(vec![(1, 0), (2, 0)]),
            vec![delta(vec![ByHash(1), Insert(1, 0), ByHash(2)])],
        ),
        // A removal then an update of an immutable component.
        one(
            APP_IMMUTABLE,
            Entries(vec![(1, 1)]),
            vec![Remove, delta(vec![Insert(2, 1)])],
        ),
        one(
            ComponentId::GROUP_ROOT_KEY,
            Entries(vec![(0, 4)]),
            vec![Remove, OperationSpec::Scalar(4)],
        ),
        // 63/64-byte elements in a bounded map, 16383/16384-byte ones in an
        // unbounded map and set.
        one(
            APP_MAP,
            Entries(vec![(14, 4)]),
            vec![
                delta(vec![Insert(15, 5), Update(14, 5)]),
                delta(vec![Delete(14)]),
            ],
        ),
        one(
            XMTP_MAP,
            Entries(vec![(18, 9)]),
            vec![
                delta(vec![Insert(19, 10), Update(18, 10)]),
                delta(vec![Delete(19)]),
            ],
        ),
        one(
            XMTP_SET,
            Entries(vec![(18, 0)]),
            vec![delta(vec![Insert(19, 0), ByHash(18)])],
        ),
        // A snapshot over the bound fails; one that shrinks back under it
        // applies.
        one(APP_MAP, Heavy(8), vec![delta(vec![Insert(8, 6)])]),
        one(APP_MAP, Heavy(9), vec![delta(vec![Delete(0)])]),
        one(APP_MAP, Heavy(9), vec![delta(vec![Insert(9, 1)])]),
        // A value that is not UTF-8 fails every update that keeps it.
        one(
            APP_NAMES,
            Entries(vec![(1, 2)]),
            vec![delta(vec![Insert(2, 1)])],
        ),
        one(
            APP_NAMES,
            Entries(vec![(1, 2)]),
            vec![delta(vec![Delete(1)])],
        ),
        // Component ids order numerically across varint lengths.
        one(
            ComponentId::COMPONENT_REGISTRY,
            Entries(vec![(6, 0), (7, 0), (8, 0), (9, 0), (0, 0)]),
            vec![delta(vec![Insert(1, 0), Delete(0)])],
        ),
        one(
            ComponentId::COMPONENT_REGISTRY,
            Corrupt(vec![(6, 0), (7, 0), (8, 0), (9, 0)], Corruption::Reversed),
            vec![delta(vec![Insert(1, 0)])],
        ),
        // Malformed committed bytes fail the first update, not a removal.
        one(
            ComponentId::ADMIN_LIST,
            Corrupt(vec![(1, 0), (2, 0)], Corruption::Reversed),
            vec![delta(vec![Insert(3, 0)])],
        ),
        one(
            APP_MAP,
            Corrupt(vec![(1, 1)], Corruption::NonMinimalLength),
            vec![delta(vec![Insert(2, 1)])],
        ),
        one(
            XMTP_SET,
            Corrupt(vec![(1, 0), (2, 0)], Corruption::Truncate(5)),
            vec![Remove, delta(vec![Insert(2, 0)])],
        ),
        one(
            ComponentId::COMPONENT_REGISTRY,
            Corrupt(vec![(0, 0)], Corruption::Trailing(0)),
            vec![delta(vec![Insert(1, 0)])],
        ),
    ];
    // A sender drops a hash removal after it applies; the index must hold
    // the key again for the next one.
    let dropped = Case {
        subjects: vec![(
            subject(ComponentId::ADMIN_LIST),
            Entries(vec![(1, 0), (2, 0)]),
        )],
        proposals: vec![
            (0, delta(vec![ByHash(1)]), true),
            (0, delta(vec![ByHash(1)]), false),
        ],
    };
    for case in cases.iter().chain([&dropped]) {
        if let Err(error) = check(case) {
            panic!("{case:?}: {error}");
        }
    }
}
