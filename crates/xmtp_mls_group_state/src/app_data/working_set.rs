//! The AppData components one commit touches, each decoded once.
//!
//! A [`WorkingSet`] holds the state of every component that one commit's
//! `AppDataUpdate` proposals touch. It applies each `Update` payload in
//! memory and encodes each touched component once at the end, so the work
//! of a commit is linear in the bytes it carries and the state it touches,
//! however its changes are split across proposals. The byte-level
//! [`apply_app_data_update_payload`] decodes and encodes the whole
//! component for each proposal instead.
//!
//! ## Same verdict, same error class
//!
//! Members must agree on every commit, so for every input an update returns
//! the result [`apply_app_data_update_payload`] returns for the same prior
//! value: the same new value, or an error of the same
//! [`ComponentSourceError`] variant. Each update runs the same checks in the
//! same order:
//!
//! 1. the element bound on the payload alone,
//! 2. immutability: an immutable component that is present is not
//!    overwritten,
//! 3. the payload decode and, for `COMPONENT_REGISTRY`, the registry's write
//!    invariants,
//! 4. the decode of the committed value, on the component's first update,
//! 5. the delta, mutation by mutation,
//! 6. UTF-8 values in a `TLS_MAP_INBOX_ID_STRING` map,
//! 7. the snapshot bound, then the element bound, on a field-bounded
//!    component,
//! 8. the group-root length.
//!
//! A snapshot over its bound after any one proposal fails that proposal,
//! even when a later proposal would shrink it. Error text is not part of a
//! verdict, and a few messages are shorter than the byte-level ones.
//!
//! ## Lazy decoding and type resolution
//!
//! A component's committed bytes are decoded when an update or a read first
//! needs its entries. [`WorkingSet::remove`] never decodes them and never
//! resolves the component's type, as staging does not today. An update
//! resolves the type exactly where [`apply_app_data_update_payload`] does:
//! for the payload bound of a bounded component, and for the apply of a
//! component with no built-in `Component` impl. A caller that resolves a
//! type where today's code does, such as validation of a `Remove`, calls
//! [`WorkingSet::component_type`]. The registry is the pre-commit one for
//! the whole commit, so each type is resolved once.
//!
//! ## `RemoveByHash`
//!
//! A set's hash index is built on the first `RemoveByHash` and kept current
//! as keys come and go. Every hash in a delta resolves against the set
//! before that delta and before any of its mutations, and a resolution
//! failure comes first, as in `TlsSet::apply_delta`: `[Insert(k),
//! RemoveByHash(h(k))]` with `k` absent fails with `KeyNotFound`.
//!
//! ## Atomic updates and checkpoints
//!
//! An update that fails leaves the working set as it was. A
//! [`WorkingSet::checkpoint`] marks a point that [`WorkingSet::rollback`]
//! returns to, undoing every update and removal since, including the
//! entries, the hash index, the snapshot length, and the counts. A sender
//! uses one around each pending proposal it considers keeping.
//!
//! ## Registry reads
//!
//! [`WorkingSet::get`] returns an entry's raw bytes. A registry entry that
//! the registry does not recognize still has bytes, so read registry
//! policy through [`WorkingSet::registry_entry`], which hides it as
//! [`ComponentRegistry::get`] does.
//!
//! ## Memory
//!
//! A decoded collection keeps its committed entries in the committed bytes,
//! as ranges, and the values the commit writes in one byte arena, so a map
//! of `n` committed entries costs about `16n` bytes beyond its bytes.

use std::{
    borrow::Cow,
    collections::BTreeMap,
    sync::atomic::{AtomicUsize, Ordering},
};

use openmls::messages::proposals::AppDataUpdateOperation;
use tls_codec::{Deserialize, Serialize, VLBytes};
use xmtp_cryptography::secret::Zeroizing;
use xmtp_mls_tls_collections::{
    tls_map::{TlsMapDelta, TlsMapError, TlsMapMutation},
    tls_set::{TlsKeyHash, TlsSetDelta, TlsSetError, TlsSetMutation},
};
use xmtp_proto::xmtp::mls::message_contents::{ComponentMetadata, ComponentType};

#[cfg(doc)]
use crate::app_data::component_source::apply_app_data_update_payload;
use crate::{
    app_data::{
        component_id::ComponentId,
        component_registry::ComponentRegistry,
        component_source::{
            ComponentSourceError, MAX_FIELD_ELEMENT_BYTES, MAX_FIELD_SNAPSHOT_BYTES,
            check_field_bounds, check_group_root_length, check_payload_element_bound,
            component_type as well_known_type, has_payload_bound, is_field_bounded,
            registered_type, registry_type_tag,
        },
        components::{
            tls_map_components::validate_registry_delta,
            type_dispatch::apply_update_payload_for_type,
        },
        registry_table::lookup_component,
        typed::{ComponentTypedError, ErasedComponent},
        validation::ComponentWriteValidator,
    },
    inbox_id::InboxId,
};

mod tracked_map;

use tracked_map::{KeyKind, MapShape, Measured, Replaced, Span, TrackedMap};

/// The longest vector body tls_codec encodes with its `mls` feature.
const MAX_VECTOR_BODY: usize = (1 << 30) - 1;

/// Tells working sets apart, so a checkpoint ends only in the set that
/// took it.
static NEXT_SET: AtomicUsize = AtomicUsize::new(0);

/// A component's committed bytes, or `None` when it is absent.
type Base<'a> = Box<dyn Fn(ComponentId) -> Option<&'a [u8]> + 'a>;

/// Each written component's state: its bytes, or `None` once removed. A
/// state can be the commit-log signer's private key, so each is wiped on
/// drop.
pub type ComponentStates = BTreeMap<ComponentId, Option<Zeroizing<Vec<u8>>>>;

/// The AppData components one commit's proposals touch, each decoded once.
///
/// Built from the pre-commit registry and a closure that returns a
/// component's bytes before the commit: the group's extensions, a batch's
/// states, or the dictionary with pending proposals applied. Apply each
/// proposal in order with [`Self::apply`], then take the touched states
/// with [`Self::into_states`]. See the [module docs](self) for the
/// guarantees.
pub struct WorkingSet<'a> {
    /// Tells this set's checkpoints from another set's.
    id: usize,
    registry: Cow<'a, ComponentRegistry>,
    base: Base<'a>,
    /// The registry type tag of each component without a well-known type,
    /// once read: `None` when it has no recognized entry. A type that does
    /// not resolve keeps its tag too, so each entry decodes at most once.
    tags: BTreeMap<ComponentId, Option<i32>>,
    components: BTreeMap<ComponentId, Tracked<'a>>,
    undo: Vec<Undo<'a>>,
    /// Live checkpoints. With none, a successful update keeps no undo entries.
    checkpoints: usize,
}

/// A point that [`WorkingSet::rollback`] returns to.
///
/// End checkpoints in the reverse order they were taken, each in the set
/// that took it. Ending another set's checkpoint, or one taken before a
/// checkpoint that is still live, is a bug: debug builds panic, and other
/// builds ignore the call. A checkpoint that is never ended keeps its
/// changes, as [`WorkingSet::release`] does, but the set then keeps undo
/// records of every later change until it is dropped.
#[must_use = "roll back or release a checkpoint"]
#[derive(Debug)]
pub struct Checkpoint {
    /// The set that took it.
    set: usize,
    /// How many checkpoints were live when it was taken.
    depth: usize,
    /// The undo log's length when it was taken.
    mark: usize,
}

/// A component the working set holds.
struct Tracked<'a> {
    state: State<'a>,
    /// Whether a proposal wrote the component. Only written components
    /// reach [`WorkingSet::into_states`]; a decoded but unwritten one is
    /// kept so it is not decoded again.
    touched: bool,
}

enum State<'a> {
    /// Removed by a proposal. A `Remove` resolves no type, so its shape is
    /// unknown until the next update.
    Removed,
    /// A scalar, written by a proposal. It can be the commit-log signer's
    /// private key.
    Scalar(Zeroizing<Vec<u8>>),
    Collection(TrackedMap<'a>),
}

/// How to undo one change to the working set.
enum Undo<'a> {
    /// The component had no record.
    Created(ComponentId),
    /// The component's record before a removal or a scalar write.
    Replaced(ComponentId, Box<Tracked<'a>>),
    /// A collection's flags before an update.
    Flags {
        id: ComponentId,
        touched: bool,
        present: bool,
    },
    /// A collection entry before a write.
    Entry(ComponentId, Replaced),
    /// A collection's arena length before an update.
    Arena(ComponentId, usize),
}

/// How a component's payloads apply, and how a codec failure is reported:
/// a built-in `Component` impl reports a bare codec error, and the
/// type-aware fallback wraps it with the component id.
#[derive(Clone, Copy)]
enum Codec {
    Known(&'static dyn ErasedComponent),
    Typed(ComponentType),
}

/// One mutation of a decoded delta, its key held as [`KeyKind`] bytes.
enum Mutation {
    Insert(Box<[u8]>, VLBytes),
    Update(Box<[u8]>, VLBytes),
    Delete(Box<[u8]>),
    DeleteByHash(TlsKeyHash),
}

/// A mutation with its hash resolved: the hash a deletion was named by.
enum Change {
    Insert(Box<[u8]>, VLBytes),
    Update(Box<[u8]>, VLBytes),
    Delete(Box<[u8]>, Option<TlsKeyHash>),
}

impl<'a> WorkingSet<'a> {
    /// A working set over the pre-commit `registry` and `base`, which
    /// returns a component's bytes before the commit.
    pub fn new(
        registry: Cow<'a, ComponentRegistry>,
        base: impl Fn(ComponentId) -> Option<&'a [u8]> + 'a,
    ) -> Self {
        Self {
            id: NEXT_SET.fetch_add(1, Ordering::Relaxed),
            registry,
            base: Box::new(base),
            tags: BTreeMap::new(),
            components: BTreeMap::new(),
            undo: Vec::new(),
            checkpoints: 0,
        }
    }

    /// The pre-commit registry every type resolves against.
    pub fn registry(&self) -> &ComponentRegistry {
        &self.registry
    }

    /// The type of component `id`, as `resolved_type` gives it: its
    /// well-known type, else the type its registry entry names. Each
    /// registry entry decodes at most once, whether or not its type
    /// resolves.
    pub fn component_type(
        &mut self,
        id: ComponentId,
    ) -> Result<ComponentType, ComponentSourceError> {
        if let Some(ty) = well_known_type(id) {
            return Ok(ty);
        }
        let tag = match self.tags.get(&id) {
            Some(tag) => *tag,
            None => {
                let tag = registry_type_tag(id, &mut ComponentWriteValidator::new(&self.registry))?;
                self.tags.insert(id, tag);
                tag
            }
        };
        registered_type(id, tag)
    }

    /// Apply one proposal's operation: [`Self::update`] or [`Self::remove`].
    pub fn apply(
        &mut self,
        id: ComponentId,
        operation: &AppDataUpdateOperation,
    ) -> Result<(), ComponentSourceError> {
        match operation {
            AppDataUpdateOperation::Update(payload) => self.update(id, payload.as_slice()),
            AppDataUpdateOperation::Remove => {
                self.remove(id);
                Ok(())
            }
        }
    }

    /// Apply an `Update` payload to component `id`'s state after the
    /// proposals applied so far, with the checks and the result of
    /// [`apply_app_data_update_payload`]. On failure the working set is
    /// unchanged.
    pub fn update(&mut self, id: ComponentId, payload: &[u8]) -> Result<(), ComponentSourceError> {
        let mark = self.undo.len();
        let result = self.try_update(id, payload);
        if result.is_err() {
            self.undo_to(mark);
        }
        if self.checkpoints == 0 {
            self.undo.clear();
        }
        result
    }

    /// Remove component `id`. A later update starts from no value. Neither
    /// decodes the committed bytes nor resolves the type.
    pub fn remove(&mut self, id: ComponentId) {
        let removed = Tracked {
            state: State::Removed,
            touched: true,
        };
        let previous = self.components.insert(id, removed);
        if self.checkpoints > 0 {
            self.undo.push(match previous {
                Some(record) => Undo::Replaced(id, Box::new(record)),
                None => Undo::Created(id),
            });
        }
    }

    /// Whether component `id` is present after the proposals applied so far.
    pub fn present(&self, id: ComponentId) -> bool {
        match self.components.get(&id) {
            None => (self.base)(id).is_some(),
            Some(record) => match &record.state {
                State::Removed => false,
                State::Scalar(_) => true,
                State::Collection(map) => map.present,
            },
        }
    }

    /// The serialized length of component `id` after the proposals applied
    /// so far, without encoding it; `None` when it is absent.
    pub fn snapshot_len(&self, id: ComponentId) -> Option<usize> {
        match self.components.get(&id) {
            None => (self.base)(id).map(<[u8]>::len),
            Some(record) => match &record.state {
                State::Removed => None,
                State::Scalar(value) => Some(value.len()),
                State::Collection(map) => map.present.then(|| map.snapshot_len()),
            },
        }
    }

    /// The bytes of component `id` after the proposals applied so far;
    /// `None` when it is absent. Encodes a changed collection.
    pub fn encode(
        &self,
        id: ComponentId,
    ) -> Result<Option<Zeroizing<Vec<u8>>>, ComponentSourceError> {
        Ok(match self.components.get(&id) {
            None => (self.base)(id).map(|bytes| Zeroizing::new(bytes.to_vec())),
            Some(record) => match &record.state {
                State::Removed => None,
                State::Scalar(value) => Some(value.clone()),
                State::Collection(map) if map.present => Some(Zeroizing::new(map.encode()?)),
                State::Collection(_) => None,
            },
        })
    }

    /// The value of the entry with the TLS-encoded `key` in collection
    /// `id`, after the proposals applied so far; `None` when the component
    /// or the entry is absent. A set element's value is empty. Decodes the
    /// committed bytes on first use. A registry entry's value is its raw
    /// bytes, even when the registry does not recognize the entry; read
    /// registry policy with [`Self::registry_entry`].
    pub fn get(
        &mut self,
        id: ComponentId,
        key: &[u8],
    ) -> Result<Option<&[u8]>, ComponentSourceError> {
        let codec = self.codec(id)?;
        let shape = codec
            .shape(id)
            .ok_or(ComponentSourceError::MismatchedMutation(id))?;
        let key = key_bytes(shape.key, key).map_err(|e| codec.error(id, e))?;
        self.load(id, shape, codec)?;
        Ok(self.collection(id).and_then(|map| map.get(&key)))
    }

    /// The registry entry of component `id` after the proposals applied so
    /// far, as [`ComponentRegistry::get`] reads it from that registry:
    /// `None` when the entry is absent or the registry does not recognize
    /// it.
    pub fn registry_entry(
        &mut self,
        id: ComponentId,
    ) -> Result<Option<ComponentMetadata>, ComponentSourceError> {
        let key = id.tls_serialize_detached()?;
        Ok(self
            .get(ComponentId::COMPONENT_REGISTRY, &key)?
            .and_then(|raw| ComponentRegistry::decode_recognized(id, raw).ok()))
    }

    /// The element of set `id` that `hash` names, after the proposals
    /// applied so far: the 32 bytes of an inbox id or the bytes of a byte
    /// string, as `expand_app_data_update_to_changes` reports a
    /// `RemoveByHash`. `None` when the set is absent, holds no such element,
    /// or `id` is not a set. Resolves the type first, decodes the committed
    /// bytes and builds the hash index on first use, and fails as that
    /// expansion does when one of them fails.
    pub fn resolve_hash(
        &mut self,
        id: ComponentId,
        hash: &TlsKeyHash,
    ) -> Result<Option<Vec<u8>>, ComponentSourceError> {
        let codec = self.codec(id)?;
        let Some(shape) = codec.shape(id).filter(|shape| shape.set) else {
            return Ok(None);
        };
        if !self.present(id) {
            return Ok(None);
        }
        self.load(id, shape, codec)?;
        let Some(map) = self.collection_mut(id) else {
            return Ok(None);
        };
        map.ensure_index()
            .map_err(ComponentSourceError::TlsSetApply)?;
        Ok(map.indexed(hash).map(Vec::from))
    }

    /// The components a proposal wrote, in id order.
    pub fn touched(&self) -> impl Iterator<Item = ComponentId> + '_ {
        self.components
            .iter()
            .filter(|(_, record)| record.touched)
            .map(|(id, _)| *id)
    }

    /// The state of each component a proposal wrote: its bytes, or `None`
    /// once removed. Each collection is encoded once here.
    pub fn into_states(self) -> Result<ComponentStates, ComponentSourceError> {
        self.components
            .into_iter()
            .filter(|(_, record)| record.touched)
            .map(|(id, record)| {
                let state = match record.state {
                    State::Removed => None,
                    State::Scalar(value) => Some(value),
                    State::Collection(map) if map.present => Some(Zeroizing::new(map.encode()?)),
                    State::Collection(_) => None,
                };
                Ok((id, state))
            })
            .collect()
    }

    /// Mark a point to return to.
    pub fn checkpoint(&mut self) -> Checkpoint {
        let checkpoint = Checkpoint {
            set: self.id,
            depth: self.checkpoints,
            mark: self.undo.len(),
        };
        self.checkpoints += 1;
        checkpoint
    }

    /// Undo every update and removal since `checkpoint`.
    pub fn rollback(&mut self, checkpoint: Checkpoint) {
        if self.end(&checkpoint) {
            self.undo_to(checkpoint.mark);
            if self.checkpoints == 0 {
                self.undo.clear();
            }
        }
    }

    /// Keep every change since `checkpoint`.
    pub fn release(&mut self, checkpoint: Checkpoint) {
        if self.end(&checkpoint) && self.checkpoints == 0 {
            self.undo.clear();
        }
    }

    /// End `checkpoint` when it is this set's innermost live checkpoint.
    fn end(&mut self, checkpoint: &Checkpoint) -> bool {
        let innermost = checkpoint.set == self.id && checkpoint.depth + 1 == self.checkpoints;
        debug_assert!(
            innermost,
            "end a working set's innermost checkpoint, in that set"
        );
        if innermost {
            self.checkpoints -= 1;
        }
        innermost
    }

    // implements: META-010, META-068
    fn try_update(&mut self, id: ComponentId, payload: &[u8]) -> Result<(), ComponentSourceError> {
        // The payload alone, first, so the verdict does not depend on the
        // state the update applies to.
        if has_payload_bound(id) {
            let ty = self.component_type(id)?;
            check_payload_element_bound(id, ty, payload)?;
        }
        // Only an overwrite: the bootstrap commit writes immutable seeds.
        if id.is_immutable() && self.present(id) {
            return Err(ComponentSourceError::ImmutableUpdate(id));
        }
        let codec = self.codec(id)?;
        match codec.shape(id) {
            Some(shape) => self.update_collection(id, shape, codec, payload),
            None => self.update_scalar(id, codec, payload),
        }
    }

    fn update_scalar(
        &mut self,
        id: ComponentId,
        codec: Codec,
        payload: &[u8],
    ) -> Result<(), ComponentSourceError> {
        let prior = match self.components.get(&id).map(|record| &record.state) {
            None => (self.base)(id),
            Some(State::Scalar(value)) => Some(value.as_slice()),
            Some(State::Removed) => None,
            Some(State::Collection(_)) => return Err(ComponentSourceError::MismatchedMutation(id)),
        };
        // The new value can be the commit-log signer's private key.
        let value = Zeroizing::new(codec.apply_scalar(id, payload, prior)?);
        if is_field_bounded(id) {
            check_field_bounds(id, self.component_type(id)?, &value)?;
        }
        check_group_root_length(id, &value)?;
        let record = Tracked {
            state: State::Scalar(value),
            touched: true,
        };
        let undo = match self.components.insert(id, record) {
            Some(previous) => Undo::Replaced(id, Box::new(previous)),
            None => Undo::Created(id),
        };
        self.undo.push(undo);
        Ok(())
    }

    // implements: META-013
    fn update_collection(
        &mut self,
        id: ComponentId,
        shape: MapShape,
        codec: Codec,
        payload: &[u8],
    ) -> Result<(), ComponentSourceError> {
        let mutations = decode_delta(id, shape, codec, payload)?;
        self.load(id, shape, codec)?;
        let Self {
            components, undo, ..
        } = self;
        let Some(Tracked {
            state: State::Collection(map),
            touched,
        }) = components.get_mut(&id)
        else {
            return Err(ComponentSourceError::MismatchedMutation(id));
        };
        undo.push(Undo::Flags {
            id,
            touched: *touched,
            present: map.present,
        });
        undo.push(Undo::Arena(id, map.arena_len()));
        let apply_error = |error| {
            if shape.set {
                ComponentSourceError::TlsSetApply(TlsSetError::Map(error))
            } else {
                ComponentSourceError::TlsMapApply(error)
            }
        };

        // Resolve every hash against the set before the delta, before any
        // mutation, so a resolution failure comes first.
        if mutations
            .iter()
            .any(|mutation| matches!(mutation, Mutation::DeleteByHash(_)))
        {
            map.ensure_index()
                .map_err(ComponentSourceError::TlsSetApply)?;
        }
        let changes = mutations
            .into_iter()
            .map(|mutation| {
                Ok(match mutation {
                    Mutation::Insert(key, value) => Change::Insert(key, value),
                    Mutation::Update(key, value) => Change::Update(key, value),
                    Mutation::Delete(key) => Change::Delete(key, None),
                    Mutation::DeleteByHash(hash) => {
                        let key = map
                            .indexed(&hash)
                            .ok_or_else(|| apply_error(TlsMapError::KeyNotFound))?;
                        Change::Delete(key, Some(hash))
                    }
                })
            })
            .collect::<Result<Vec<_>, ComponentSourceError>>()?;

        for change in changes {
            let (key, value, hash) = match change {
                Change::Insert(key, value) if map.get(&key).is_none() => (key, Some(value), None),
                Change::Insert(..) => return Err(apply_error(TlsMapError::KeyExists)),
                Change::Update(key, value) if map.get(&key).is_some() => (key, Some(value), None),
                Change::Delete(key, hash) if map.get(&key).is_some() => (key, None, hash),
                Change::Update(..) | Change::Delete(..) => {
                    return Err(apply_error(TlsMapError::KeyNotFound));
                }
            };
            let value = value.map(|value| {
                if shape.set {
                    Span::EMPTY
                } else {
                    map.push(value.as_slice())
                }
            });
            undo.push(Undo::Entry(id, map.write(key, value, hash)));
        }
        *touched = true;
        map.present = true;

        if map.body_len() > MAX_VECTOR_BODY {
            return Err(codec.error(id, tls_codec::Error::InvalidVectorLength));
        }
        if map.non_utf8() > 0 {
            return Err(ComponentSourceError::MalformedComponentValue {
                component_id: id,
                reason: format!("not valid UTF-8: {} values", map.non_utf8()),
            });
        }
        if is_field_bounded(id) {
            let len = map.snapshot_len();
            if len > MAX_FIELD_SNAPSHOT_BYTES {
                return Err(ComponentSourceError::FieldBoundExceeded {
                    component_id: id,
                    len,
                    max: MAX_FIELD_SNAPSHOT_BYTES,
                });
            }
            if let Some(len) = map.longest_oversized() {
                return Err(ComponentSourceError::FieldBoundExceeded {
                    component_id: id,
                    len,
                    max: MAX_FIELD_ELEMENT_BYTES,
                });
            }
        }
        Ok(())
    }

    /// How component `id`'s payloads apply: its built-in `Component` impl,
    /// else its resolved type.
    fn codec(&mut self, id: ComponentId) -> Result<Codec, ComponentSourceError> {
        match lookup_component(id) {
            Some(component) => Ok(Codec::Known(component)),
            None => Ok(Codec::Typed(self.component_type(id)?)),
        }
    }

    /// Decode collection `id` on first use: its committed bytes, or no
    /// entries once it is absent or removed.
    fn load(
        &mut self,
        id: ComponentId,
        shape: MapShape,
        codec: Codec,
    ) -> Result<(), ComponentSourceError> {
        match self.components.get_mut(&id) {
            Some(record) => {
                if matches!(record.state, State::Removed) {
                    record.state = State::Collection(TrackedMap::absent(shape));
                }
            }
            None => {
                let map = match (self.base)(id) {
                    Some(bytes) => {
                        TrackedMap::decode(shape, bytes).map_err(|e| codec.error(id, e))?
                    }
                    None => TrackedMap::absent(shape),
                };
                let record = Tracked {
                    state: State::Collection(map),
                    touched: false,
                };
                self.components.insert(id, record);
            }
        }
        Ok(())
    }

    fn collection(&self, id: ComponentId) -> Option<&TrackedMap<'a>> {
        match &self.components.get(&id)?.state {
            State::Collection(map) => Some(map),
            _ => None,
        }
    }

    fn collection_mut(&mut self, id: ComponentId) -> Option<&mut TrackedMap<'a>> {
        match &mut self.components.get_mut(&id)?.state {
            State::Collection(map) => Some(map),
            _ => None,
        }
    }

    fn undo_to(&mut self, mark: usize) {
        while self.undo.len() > mark {
            let Some(entry) = self.undo.pop() else {
                break;
            };
            match entry {
                Undo::Created(id) => {
                    self.components.remove(&id);
                }
                Undo::Replaced(id, record) => {
                    self.components.insert(id, *record);
                }
                Undo::Flags {
                    id,
                    touched,
                    present,
                } => {
                    if let Some(record) = self.components.get_mut(&id) {
                        record.touched = touched;
                        if let State::Collection(map) = &mut record.state {
                            map.present = present;
                        }
                    }
                }
                Undo::Entry(id, replaced) => {
                    if let Some(map) = self.collection_mut(id) {
                        map.restore(replaced);
                    }
                }
                Undo::Arena(id, len) => {
                    if let Some(map) = self.collection_mut(id) {
                        map.truncate_arena(len);
                    }
                }
            }
        }
    }
}

impl Codec {
    /// A codec failure, reported as today's apply reports it. The
    /// type-aware apply of a string map returns its codec errors before it
    /// wraps them, so they stay bare.
    fn error(self, id: ComponentId, error: tls_codec::Error) -> ComponentSourceError {
        match self {
            Self::Typed(ty) if ty != ComponentType::TlsMapInboxIdString => {
                ComponentSourceError::MalformedComponentValue {
                    component_id: id,
                    reason: format!("type-aware apply ({ty:?}) tls codec error: {error}"),
                }
            }
            Self::Known(_) | Self::Typed(_) => ComponentSourceError::TlsCodec(error),
        }
    }

    /// The collection shape of component `id`, or `None` for a scalar.
    fn shape(self, id: ComponentId) -> Option<MapShape> {
        let (ty, typed) = match self {
            Self::Known(component) => (component.component_type(), false),
            Self::Typed(ty) => (ty, true),
        };
        let (key, set, measured) = match ty {
            ComponentType::Bytes | ComponentType::String | ComponentType::Unspecified => {
                return None;
            }
            ComponentType::TlsSetInboxId => (KeyKind::InboxId, true, Measured::Nothing),
            ComponentType::TlsSetBytes => (KeyKind::Bytes, true, Measured::Keys),
            ComponentType::TlsMapInboxIdBytes | ComponentType::TlsMapInboxIdString => {
                (KeyKind::InboxId, false, Measured::Values)
            }
            ComponentType::TlsMapBytesBytes if id == ComponentId::COMPONENT_REGISTRY => {
                (KeyKind::ComponentId, false, Measured::Values)
            }
            ComponentType::TlsMapBytesBytes => (KeyKind::Bytes, false, Measured::KeysAndValues),
        };
        Some(MapShape {
            key,
            set,
            // The type-aware apply checks values; no built-in impl is a
            // string map.
            utf8: typed && ty == ComponentType::TlsMapInboxIdString,
            // Only the registry's built-in impl holds a delta to the
            // registry's write invariants.
            registry: !typed && id == ComponentId::COMPONENT_REGISTRY,
            measured,
        })
    }

    /// Apply a scalar payload with today's byte-level codec, which reads
    /// the payload alone.
    fn apply_scalar(
        self,
        id: ComponentId,
        payload: &[u8],
        prior: Option<&[u8]>,
    ) -> Result<Vec<u8>, ComponentTypedError> {
        match self {
            Self::Known(component) => component.apply_update_payload(payload, prior),
            Self::Typed(ty) => apply_update_payload_for_type(id, ty, payload, prior),
        }
    }
}

/// Decode a delta payload with tls_codec's own codec, and hold a
/// `COMPONENT_REGISTRY` delta to the registry's write invariants.
fn decode_delta(
    id: ComponentId,
    shape: MapShape,
    codec: Codec,
    payload: &[u8],
) -> Result<Vec<Mutation>, ComponentSourceError> {
    let codec_error = |e| codec.error(id, e);
    let inbox_key = |key: InboxId| Ok(Box::<[u8]>::from(key.as_bytes().as_slice()));
    let bytes_key = |key: VLBytes| Ok(Vec::from(key).into_boxed_slice());
    match (shape.key, shape.set) {
        (KeyKind::InboxId, true) => {
            set_mutations(TlsSetDelta::tls_deserialize_exact(payload), inbox_key)
        }
        (KeyKind::Bytes, true) => {
            set_mutations(TlsSetDelta::tls_deserialize_exact(payload), bytes_key)
        }
        (KeyKind::InboxId, false) => {
            map_mutations(TlsMapDelta::tls_deserialize_exact(payload), inbox_key)
        }
        (KeyKind::Bytes, false) => {
            map_mutations(TlsMapDelta::tls_deserialize_exact(payload), bytes_key)
        }
        (KeyKind::ComponentId, false) => {
            let delta = TlsMapDelta::<ComponentId, VLBytes>::tls_deserialize_exact(payload)
                .map_err(codec_error)?;
            if shape.registry {
                validate_registry_delta(&delta)?;
            }
            map_mutations(Ok(delta), |key: ComponentId| {
                Ok(key.tls_serialize_detached()?.into_boxed_slice())
            })
        }
        (KeyKind::ComponentId, true) => return Err(ComponentSourceError::MismatchedMutation(id)),
    }
    .map_err(codec_error)
}

fn set_mutations<K>(
    delta: Result<TlsSetDelta<K>, tls_codec::Error>,
    key: impl Fn(K) -> Result<Box<[u8]>, tls_codec::Error>,
) -> Result<Vec<Mutation>, tls_codec::Error> {
    delta?
        .mutations
        .into_iter()
        .map(|mutation| {
            Ok(match mutation {
                TlsSetMutation::Insert(k) => Mutation::Insert(key(k)?, VLBytes::new(Vec::new())),
                TlsSetMutation::Remove(k) => Mutation::Delete(key(k)?),
                TlsSetMutation::RemoveByHash(hash) => Mutation::DeleteByHash(hash),
            })
        })
        .collect()
}

fn map_mutations<K>(
    delta: Result<TlsMapDelta<K, VLBytes>, tls_codec::Error>,
    key: impl Fn(K) -> Result<Box<[u8]>, tls_codec::Error>,
) -> Result<Vec<Mutation>, tls_codec::Error> {
    delta?
        .mutations
        .into_iter()
        .map(|mutation| {
            Ok(match mutation {
                TlsMapMutation::Insert { key: k, value } => Mutation::Insert(key(k)?, value),
                TlsMapMutation::Update { key: k, value } => Mutation::Update(key(k)?, value),
                TlsMapMutation::Delete { key: k } => Mutation::Delete(key(k)?),
            })
        })
        .collect()
}

/// A TLS-encoded key as [`KeyKind`] bytes.
fn key_bytes(kind: KeyKind, key: &[u8]) -> Result<Box<[u8]>, tls_codec::Error> {
    Ok(match kind {
        KeyKind::InboxId => InboxId::tls_deserialize_exact(key)?
            .as_bytes()
            .as_slice()
            .into(),
        KeyKind::ComponentId => ComponentId::tls_deserialize_exact(key)?
            .tls_serialize_detached()?
            .into_boxed_slice(),
        KeyKind::Bytes => Vec::from(VLBytes::tls_deserialize_exact(key)?).into_boxed_slice(),
    })
}

/// Test-only operation counters and a hash clash, per thread.
#[cfg(test)]
mod counters {
    use std::cell::Cell;

    use xmtp_mls_tls_collections::tls_set::TlsKeyHash;

    thread_local! {
        static HASHES: Cell<usize> = const { Cell::new(0) };
        static DECODES: Cell<usize> = const { Cell::new(0) };
        static CLASH: Cell<bool> = const { Cell::new(false) };
    }

    /// Run `f` with every key hash that [`key_hash`] passes replaced by one
    /// value, so any two keys share a hash.
    pub(super) fn with_hash_clash<R>(f: impl FnOnce() -> R) -> R {
        struct Reset;
        impl Drop for Reset {
            fn drop(&mut self) {
                CLASH.set(false);
            }
        }
        CLASH.set(true);
        let _reset = Reset;
        f()
    }

    /// `hash`, or inside [`with_hash_clash`] the one hash every key has.
    pub(super) fn key_hash(hash: TlsKeyHash) -> TlsKeyHash {
        if CLASH.get() {
            TlsKeyHash::from_bytes([0; 32])
        } else {
            hash
        }
    }

    pub(super) fn count_hash() {
        HASHES.with(|count| count.set(count.get() + 1));
    }

    pub(super) fn count_decode() {
        DECODES.with(|count| count.set(count.get() + 1));
    }

    /// SHA-256 key hashes so far on this thread.
    pub(super) fn hashes() -> usize {
        HASHES.with(Cell::get)
    }

    /// Committed collection decodes so far on this thread.
    pub(super) fn decodes() -> usize {
        DECODES.with(Cell::get)
    }
}

#[cfg(test)]
mod tests;
