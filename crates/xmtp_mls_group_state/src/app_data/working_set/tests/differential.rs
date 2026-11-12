//! A [`WorkingSet`] against the frozen byte-level apply in
//! [`super::reference`], on random committed values and proposal
//! sequences: the same verdict, error class, lengths, and final bytes
//! after every proposal, in a receiver's commit and in a sender's
//! keep-or-roll-back loop. A receiver's commit is also held to
//! production's byte-level apply, and the working set's reads to the reads
//! they replace.

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
use tls_codec::{Deserialize, Serialize, Size, VLBytes};
use xmtp_cryptography::secret::Zeroizing;
use xmtp_mls_tls_collections::{
    tls_map::{TlsMap, TlsMapDelta, TlsMapEntry, TlsMapError},
    tls_set::{TlsKeyHash, TlsSet, TlsSetDelta, TlsSetError, TlsSetMutation},
};
use xmtp_proto::xmtp::mls::message_contents::ComponentType;

use super::{WorkingSet, counters, metadata, reference, registry, update};
use crate::{
    app_data::{
        component_id::ComponentId,
        component_registry::ComponentRegistry,
        component_source::{
            ComponentSourceError, apply_app_data_update_payload, expand_app_data_update_to_changes,
        },
        registry_table::WELL_KNOWN,
    },
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

/// How the generator writes component `id`, one of [`SUBJECTS`].
fn kind_of(id: ComponentId) -> Kind {
    SUBJECTS
        .iter()
        .find(|(subject, _)| *subject == id)
        .map(|(_, kind)| *kind)
        .unwrap()
}

/// The registry the subjects resolve against.
fn subject_registry() -> ComponentRegistry {
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
            MutationSpec::ByHash(k) => {
                delta.remove_by_hash(counters::key_hash(TlsKeyHash::of(&key(k)).unwrap()))
            }
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

/// What a receiver's commit is compared with besides the frozen reference.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Oracles {
    /// Also production's byte-level apply, and the expansion that resolves
    /// a `RemoveByHash` for validation.
    All,
    /// The frozen reference alone: inside a hash clash, which production's
    /// hashes do not see.
    Reference,
}

/// A receiver's commit: every proposal applies in order, and the first
/// failure rejects the commit. After each proposal, the working set's
/// reads match the byte-level state.
fn check_commit(
    registry: &ComponentRegistry,
    base: &BTreeMap<ComponentId, Vec<u8>>,
    proposals: &Proposals,
    oracles: Oracles,
) -> Result<(), TestCaseError> {
    let mut set = WorkingSet::new(Cow::Borrowed(registry), |id| {
        base.get(&id).map(Vec::as_slice)
    });
    for (id, _, _) in proposals {
        prop_assert_eq!(
            set.component_type(*id).map_err(|e| class(&e)),
            reference::resolved_type(*id, registry).map_err(|e| class(&e)),
            "type of {}",
            id
        );
    }
    let mut expected = reference::States::new();
    let mut production = reference::States::new();
    for (index, (id, operation, _)) in proposals.iter().enumerate() {
        if oracles == Oracles::All {
            let prior = reference::current(&expected, base, *id);
            check_hash_removals(&mut set, registry, *id, operation, prior)?;
        }
        let want = reference::apply_one(registry, base, &mut expected, *id, operation)
            .map_err(|e| class(&e));
        let got = set.apply(*id, operation).map_err(|e| class(&e));
        prop_assert_eq!(&got, &want, "proposal {} to {}", index, id);
        if oracles == Oracles::All {
            let verdict = reference::apply_one_with(
                apply_app_data_update_payload,
                registry,
                base,
                &mut production,
                *id,
                operation,
            )
            .map_err(|e| class(&e));
            prop_assert_eq!(
                &verdict,
                &want,
                "production on proposal {} to {}",
                index,
                id
            );
        }
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
        check_reads(&mut set, registry, *id, bytes)?;
    }
    if oracles == Oracles::All {
        prop_assert_eq!(plain(production), plain(expected.clone()));
    }
    let got = set
        .into_states()
        .map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
    prop_assert_eq!(plain(got), plain(expected));
    Ok(())
}

/// Before an update, each `RemoveByHash` of a set delta resolves in the
/// working set as validation's expansion resolves it against the same
/// prior state, errors by class.
fn check_hash_removals(
    set: &mut WorkingSet<'_>,
    registry: &ComponentRegistry,
    id: ComponentId,
    operation: &AppDataUpdateOperation,
    prior: Option<&[u8]>,
) -> Result<(), TestCaseError> {
    let AppDataUpdateOperation::Update(payload) = operation else {
        return Ok(());
    };
    let removals = match kind_of(id) {
        Kind::InboxSet => hash_removals::<InboxId>(payload.as_slice()),
        Kind::BytesSet => hash_removals::<VLBytes>(payload.as_slice()),
        _ => Vec::new(),
    };
    if removals.is_empty() {
        return Ok(());
    }
    let expanded = expand_app_data_update_to_changes(id, operation, prior, registry);
    for (index, hash) in removals {
        let want = match &expanded {
            Ok(changes) => Ok(changes[index].value.clone()),
            Err(error) => Err(class(error)),
        };
        let got = set.resolve_hash(id, &hash).map_err(|e| class(&e));
        prop_assert_eq!(got, want, "hash removal {} in an update of {}", index, id);
    }
    Ok(())
}

/// The index and hash of each `RemoveByHash` in a set delta; none when
/// `payload` is not one.
fn hash_removals<K: Deserialize + Size>(payload: &[u8]) -> Vec<(usize, TlsKeyHash)> {
    let Ok(delta) = TlsSetDelta::<K>::tls_deserialize_exact(payload) else {
        return Vec::new();
    };
    delta
        .mutations
        .iter()
        .enumerate()
        .filter_map(|(index, mutation)| match mutation {
            TlsSetMutation::RemoveByHash(hash) => Some((index, *hash)),
            _ => None,
        })
        .collect()
}

/// The working set's reads of component `id` match its byte-level `bytes`:
/// the encoding, the value of each key the generator writes, and in the
/// registry, each entry the registry recognizes.
fn check_reads(
    set: &mut WorkingSet<'_>,
    registry: &ComponentRegistry,
    id: ComponentId,
    bytes: Option<&[u8]>,
) -> Result<(), TestCaseError> {
    let encoded = set
        .encode(id)
        .map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
    prop_assert_eq!(
        encoded.as_deref().map(Vec::as_slice),
        bytes,
        "encoding of {}",
        id
    );
    let ty = match reference::resolved_type(id, registry) {
        Ok(ty) => ty,
        Err(error) => {
            let read = set.get(id, &[]).map(|_| ()).map_err(|e| class(&e));
            prop_assert_eq!(read, Err(class(&error)), "read of {}", id);
            return Ok(());
        }
    };
    for key in pool_keys(kind_of(id)) {
        let got = set
            .get(id, &key)
            .map(|value| value.map(<[u8]>::to_vec))
            .map_err(|e| class(&e));
        prop_assert_eq!(got, decoded_get(id, ty, bytes, &key), "a key of {}", id);
    }
    if id == ComponentId::COMPONENT_REGISTRY {
        let after = bytes
            .map(ComponentRegistry::from_bytes)
            .transpose()
            .map_err(|e| TestCaseError::fail(format!("{e:?}")))?
            .unwrap_or_else(ComponentRegistry::new);
        for key in REGISTRY_KEYS.map(ComponentId::new) {
            let got = set
                .registry_entry(key)
                .map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
            let want = after
                .get(&key)
                .map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
            prop_assert_eq!(got, want, "registry entry {}", key);
        }
    }
    Ok(())
}

/// The TLS encoding of each key the generator writes to a component of
/// `kind`.
fn pool_keys(kind: Kind) -> Vec<Vec<u8>> {
    match kind {
        Kind::BytesMap | Kind::BytesSet | Kind::Scalar => pools()
            .keys
            .iter()
            .map(|key| vl(key).tls_serialize_detached().unwrap())
            .collect(),
        Kind::InboxMap | Kind::InboxSet => (0..INBOXES as u8)
            .map(|index| inbox(index).tls_serialize_detached().unwrap())
            .collect(),
        Kind::Registry => REGISTRY_KEYS
            .map(|key| ComponentId::new(key).tls_serialize_detached().unwrap())
            .to_vec(),
    }
}

/// The value of the TLS-encoded `key` in component `id`'s `bytes`, decoded
/// as the byte-level code decodes a component of type `ty`; a set element's
/// value is empty. Errors by class, as [`WorkingSet::get`] reports them.
fn decoded_get(
    id: ComponentId,
    ty: ComponentType,
    bytes: Option<&[u8]>,
    key: &[u8],
) -> Result<Option<Vec<u8>>, String> {
    fn in_set<K: Deserialize + Size + Ord>(
        bytes: &[u8],
        key: &[u8],
    ) -> Result<Option<Vec<u8>>, tls_codec::Error> {
        let set = TlsSet::<K>::tls_deserialize_exact(bytes)?;
        Ok(set.contains(&K::tls_deserialize_exact(key)?).then(Vec::new))
    }
    fn in_map<K: Deserialize + Size + Ord>(
        bytes: &[u8],
        key: &[u8],
    ) -> Result<Option<Vec<u8>>, tls_codec::Error> {
        let map = TlsMap::<K, VLBytes>::tls_deserialize_exact(bytes)?;
        let key = K::tls_deserialize_exact(key)?;
        Ok(map.get(&key).map(|value| value.as_slice().to_vec()))
    }
    type Read = fn(&[u8], &[u8]) -> Result<Option<Vec<u8>>, tls_codec::Error>;
    let read: Read = match ty {
        ComponentType::Bytes | ComponentType::String | ComponentType::Unspecified => {
            return Err(class(&ComponentSourceError::MismatchedMutation(id)));
        }
        _ if bytes.is_none() => return Ok(None),
        ComponentType::TlsSetInboxId => in_set::<InboxId>,
        ComponentType::TlsSetBytes => in_set::<VLBytes>,
        ComponentType::TlsMapInboxIdBytes | ComponentType::TlsMapInboxIdString => in_map::<InboxId>,
        ComponentType::TlsMapBytesBytes if id == ComponentId::COMPONENT_REGISTRY => {
            in_map::<ComponentId>
        }
        ComponentType::TlsMapBytesBytes => in_map::<VLBytes>,
    };
    read(bytes.unwrap_or_default(), key).map_err(|e| format!("undecodable: {e}"))
}

/// A sender's loop: each proposal is kept when it applies after the ones
/// kept before it and the sender does not drop it, and rolled back
/// otherwise. Every other proposal the sender would keep applies without a
/// checkpoint, so a failed update must leave the set as it was. Every
/// fourth proposal also applies under a second, inner checkpoint that is
/// released, as a receive check's checkpoint nests in a commit builder's.
/// With `outer`, the loop runs inside one more checkpoint, whose rollback
/// must restore the committed state.
fn check_keep(
    registry: &ComponentRegistry,
    base: &BTreeMap<ComponentId, Vec<u8>>,
    proposals: &Proposals,
    outer: bool,
) -> Result<(), TestCaseError> {
    let mut set = WorkingSet::new(Cow::Borrowed(registry), |id| {
        base.get(&id).map(Vec::as_slice)
    });
    let outer = outer.then(|| set.checkpoint());
    let mut kept = reference::States::new();
    for (index, (id, operation, dropped)) in proposals.iter().enumerate() {
        let mut trial = kept.clone();
        let want =
            reference::apply_one(registry, base, &mut trial, *id, operation).map_err(|e| class(&e));
        let checkpoint = (*dropped || index % 2 == 0).then(|| set.checkpoint());
        let inner = (index % 4 == 0).then(|| set.checkpoint());
        let got = set.apply(*id, operation).map_err(|e| class(&e));
        if let Some(inner) = inner {
            set.release(inner);
        }
        prop_assert_eq!(&got, &want, "proposal {} to {}", index, id);
        let keep = want.is_ok() && !dropped;
        if keep {
            kept = trial;
        }
        match checkpoint {
            Some(checkpoint) if keep => set.release(checkpoint),
            Some(checkpoint) => set.rollback(checkpoint),
            // A failed update without a checkpoint left the set as it was.
            None => {}
        }
        check_states(&set, base, &kept, proposals, index)?;
    }
    match outer {
        Some(checkpoint) => {
            set.rollback(checkpoint);
            check_states(
                &set,
                base,
                &reference::States::new(),
                proposals,
                proposals.len(),
            )?;
            prop_assert_eq!(set.touched().count(), 0);
            let got = set
                .into_states()
                .map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
            prop_assert!(got.is_empty());
        }
        None => {
            let got = set
                .into_states()
                .map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
            prop_assert_eq!(plain(got), plain(kept));
        }
    }
    Ok(())
}

/// The length, presence and bytes of each component `proposals` write
/// match `states` over `base`.
fn check_states(
    set: &WorkingSet<'_>,
    base: &BTreeMap<ComponentId, Vec<u8>>,
    states: &reference::States,
    proposals: &Proposals,
    index: usize,
) -> Result<(), TestCaseError> {
    for (id, _, _) in proposals {
        let bytes = reference::current(states, base, *id);
        prop_assert_eq!(
            set.snapshot_len(*id),
            len(bytes),
            "length of {} after proposal {}",
            id,
            index
        );
        prop_assert_eq!(set.present(*id), bytes.is_some());
        let encoded = set
            .encode(*id)
            .map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
        prop_assert_eq!(
            encoded.as_deref().map(Vec::as_slice),
            bytes,
            "bytes of {} after proposal {}",
            id,
            index
        );
    }
    Ok(())
}

fn check(case: &Case) -> Result<(), TestCaseError> {
    check_with(case, Oracles::All)
}

fn check_with(case: &Case, oracles: Oracles) -> Result<(), TestCaseError> {
    let registry = subject_registry();
    let (base, proposals) = case.build();
    check_commit(&registry, &base, &proposals, oracles)?;
    check_keep(&registry, &base, &proposals, false)?;
    check_keep(&registry, &base, &proposals, true)
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
#[xmtp_common::test(unwrap_try = true)]
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

/// A delta of `mutations` as written, not repaired.
fn unrepaired(mutations: Vec<MutationSpec>) -> OperationSpec {
    OperationSpec::Delta(mutations, false)
}

/// A case of `proposals` to component `id`, over the committed `base`.
fn one(id: ComponentId, base: BaseSpec, proposals: Vec<OperationSpec>) -> Case {
    Case {
        subjects: vec![(subject(id), base)],
        proposals: proposals.into_iter().map(|p| (0, p, false)).collect(),
    }
}

/// Run `check` on each case, naming the case that fails.
fn check_all<'c>(
    cases: impl IntoIterator<Item = &'c Case>,
    check: impl Fn(&Case) -> Result<(), TestCaseError>,
) {
    for case in cases {
        if let Err(error) = check(case) {
            panic!("{case:?}: {error}");
        }
    }
}

/// The cases the plan names, on every run: a hash removal of a key the
/// same delta inserts, a removal then an update, the length-prefix
/// boundaries, and malformed committed bytes.
// verifies: META-012, META-013
#[xmtp_common::test(unwrap_try = true)]
fn working_set_matches_byte_level_apply_on_pinned_cases() {
    use BaseSpec::{Absent, Corrupt, Entries, Heavy};
    use MutationSpec::{ByHash, Delete, Insert, Update};
    use OperationSpec::Remove;
    let delta = unrepaired;
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
        // A hash that names no key fails the delta before an earlier
        // mutation's error: KeyNotFound, not KeyExists.
        one(
            ComponentId::ADMIN_LIST,
            Entries(vec![(1, 0)]),
            vec![delta(vec![Insert(1, 0), ByHash(5)])],
        ),
        // A key that a hash removal deletes is inserted again in the same
        // delta.
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
        // Removing the committed value or key over the element bound
        // clears the bound.
        one(APP_MAP, Entries(vec![(1, 8)]), vec![delta(vec![Delete(1)])]),
        one(
            APP_SET,
            Entries(vec![(17, 0)]),
            vec![delta(vec![ByHash(17)])],
        ),
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
        // Malformed committed bytes fail the first update, not a removal,
        // and a hash removal's resolution.
        one(
            ComponentId::ADMIN_LIST,
            Corrupt(vec![(1, 0), (2, 0)], Corruption::Reversed),
            vec![delta(vec![Insert(3, 0)])],
        ),
        one(
            APP_SET,
            Corrupt(vec![(1, 0), (2, 0)], Corruption::Reversed),
            vec![delta(vec![ByHash(1)])],
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
    check_all(cases.iter().chain([&dropped]), check);
}

/// Two keys with one hash: a hash removal fails with `DuplicateHash`,
/// whether the clash is committed or a proposal makes it, as the byte-level
/// apply does, and a removal that ends the clash lets the next one apply.
#[xmtp_common::test(unwrap_try = true)]
fn hash_clash_matches_byte_level_apply() {
    use BaseSpec::Entries;
    use MutationSpec::{ByHash, Delete, Insert};
    let delta = unrepaired;
    counters::with_hash_clash(|| {
        let cases = [
            one(
                ComponentId::ADMIN_LIST,
                Entries(vec![(1, 0), (2, 0)]),
                vec![delta(vec![ByHash(1)])],
            ),
            one(
                APP_SET,
                Entries(vec![(1, 0)]),
                vec![
                    delta(vec![ByHash(1)]),
                    delta(vec![Insert(1, 0), Insert(2, 0)]),
                    delta(vec![ByHash(1)]),
                ],
            ),
            one(
                ComponentId::ADMIN_LIST,
                Entries(vec![(1, 0)]),
                vec![
                    delta(vec![ByHash(1)]),
                    delta(vec![Insert(1, 0), Insert(2, 0)]),
                    delta(vec![Delete(2)]),
                    delta(vec![ByHash(1)]),
                ],
            ),
        ];
        check_all(&cases, |case| check_with(case, Oracles::Reference));
    });
}

/// Every built-in collection component is a subject. The working set
/// applies a collection by its type, not through its impl, so a new one
/// fails here until the differential test covers it.
#[xmtp_common::test(unwrap_try = true)]
fn every_built_in_collection_is_a_subject() {
    let subjects: BTreeSet<ComponentId> = SUBJECTS.iter().map(|(id, _)| *id).collect();
    for (id, component) in WELL_KNOWN {
        let scalar = matches!(
            component.component_type(),
            ComponentType::Bytes | ComponentType::String | ComponentType::Unspecified
        );
        assert!(
            scalar || subjects.contains(id),
            "the differential test does not cover the built-in collection {id}"
        );
    }
}
