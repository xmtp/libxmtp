//! One collection component of a [`WorkingSet`](super::WorkingSet),
//! decoded once: its committed entries as ranges of the committed bytes,
//! the changes the commit's proposals made, and the counts the field
//! checks read.

use std::{
    cmp::Ordering,
    collections::{BTreeMap, HashMap, btree_map},
    iter::Peekable,
};

use tls_codec::{Deserialize, Serialize, VLByteSlice};
use xmtp_mls_tls_collections::tls_set::{TlsKeyHash, TlsSetError};

use crate::{
    app_data::{component_id::ComponentId, component_source::MAX_FIELD_ELEMENT_BYTES},
    inbox_id::{INBOX_ID_BYTE_LEN, InboxId},
};

/// How a collection's keys are encoded. A key is held as bytes whose
/// lexicographic order is the key type's order: the 32 bytes of
/// an inbox id, the minimal QUIC varint that encodes a component id, or
/// the bytes of a byte string.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum KeyKind {
    InboxId,
    ComponentId,
    Bytes,
}

/// Which elements of a collection the element bound measures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Measured {
    Nothing,
    Keys,
    Values,
    KeysAndValues,
}

impl Measured {
    fn keys(self) -> bool {
        matches!(self, Self::Keys | Self::KeysAndValues)
    }

    fn values(self) -> bool {
        matches!(self, Self::Values | Self::KeysAndValues)
    }
}

/// The encoding and checks of one collection component.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct MapShape {
    pub(super) key: KeyKind,
    /// A set: entries have no value.
    pub(super) set: bool,
    /// Every value must be UTF-8 (`TLS_MAP_INBOX_ID_STRING`).
    pub(super) utf8: bool,
    /// Deltas are held to the registry's write invariants.
    pub(super) registry: bool,
    pub(super) measured: Measured,
}

/// A range of a map's arena.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Span {
    start: usize,
    len: usize,
}

impl Span {
    /// The value of a set element, which holds no bytes.
    pub(super) const EMPTY: Self = Self { start: 0, len: 0 };
}

/// A committed entry: its key and value as ranges of the committed bytes.
/// A decode accepts at most `2^30 + 3` bytes, so every offset fits a `u32`.
#[derive(Clone, Copy)]
struct Committed {
    key_start: u32,
    key_len: u32,
    value_start: u32,
    value_len: u32,
}

/// Where an indexed key's bytes are.
enum KeyRef {
    Committed(u32),
    Owned(Box<[u8]>),
}

/// What the counts read from one value.
#[derive(Clone, Copy)]
struct Facts {
    len: usize,
    /// UTF-8, or in a map whose values need not be.
    utf8: bool,
}

/// What [`TrackedMap::write`] replaced, for [`TrackedMap::restore`].
pub(super) struct Replaced {
    key: Box<[u8]>,
    slot: Option<Option<Span>>,
    hash: Option<TlsKeyHash>,
}

/// A collection component decoded for one commit.
///
/// The committed entries stay in the committed bytes as ranges, so a map of
/// `n` entries costs `16n` bytes on top of them. Each key a proposal
/// changes goes in `changes`, with its value in `arena`. The snapshot
/// length, the elements over the element bound, and the values that are
/// not UTF-8 are counted as entries change, so the checks after each
/// proposal read counts instead of encoding the snapshot.
pub(super) struct TrackedMap<'a> {
    shape: MapShape,
    /// Whether the component is present. A delta creates an absent one.
    pub(super) present: bool,
    committed: &'a [u8],
    entries: Vec<Committed>,
    /// The keys the proposals changed: their value, or `None` once deleted.
    changes: BTreeMap<Box<[u8]>, Option<Span>>,
    /// The values `changes` holds.
    arena: Vec<u8>,
    /// The serialized length of the present entries, without the vector's
    /// length prefix.
    body_len: usize,
    /// The lengths of the measured elements over the element bound.
    oversized: BTreeMap<usize, u32>,
    /// The values that are not UTF-8, in a map whose values must be.
    non_utf8: usize,
    /// The present keys by the hash a `RemoveByHash` names: built on the
    /// first one, then kept current.
    hashes: Option<HashMap<TlsKeyHash, KeyRef>>,
}

impl<'a> TrackedMap<'a> {
    /// An absent component.
    pub(super) fn absent(shape: MapShape) -> Self {
        Self {
            shape,
            present: false,
            committed: &[],
            entries: Vec::new(),
            changes: BTreeMap::new(),
            arena: Vec::new(),
            body_len: 0,
            oversized: BTreeMap::new(),
            non_utf8: 0,
            hashes: None,
        }
    }

    /// Decode committed `bytes`, accepting exactly the bytes that
    /// `TlsMap::<K, V>::tls_deserialize_exact` accepts for the shape's key
    /// and value types: the length and element codecs are tls_codec's own.
    // implements: META-012
    pub(super) fn decode(shape: MapShape, bytes: &'a [u8]) -> Result<Self, tls_codec::Error> {
        #[cfg(any(test, feature = "test-utils"))]
        super::counters::count_decode();
        let mut rest = bytes;
        let (body_len, _) = tls_codec::vlen::read_length(&mut rest)?;
        match rest.len().cmp(&body_len) {
            Ordering::Less => return Err(tls_codec::Error::EndOfStream),
            Ordering::Greater => return Err(tls_codec::Error::TrailingData),
            Ordering::Equal => {}
        }
        let mut map = Self {
            present: true,
            committed: bytes,
            body_len,
            ..Self::absent(shape)
        };
        let mut pos = bytes.len() - rest.len();
        while pos < bytes.len() {
            let key = read_key(shape.key, bytes, &mut pos)?;
            let value = if shape.set {
                (pos, 0)
            } else {
                read_bytes(bytes, &mut pos)?
            };
            let entry = Committed {
                key_start: key.0 as u32,
                key_len: key.1 as u32,
                value_start: value.0 as u32,
                value_len: value.1 as u32,
            };
            if map
                .entries
                .last()
                .is_some_and(|last| map.key(last) >= map.key(&entry))
            {
                return Err(tls_codec::Error::DecodingError(
                    "TlsMap entries not sorted or contain duplicates".into(),
                ));
            }
            map.entries.push(entry);
        }
        // The entries live for the whole commit; growth can leave half the
        // capacity unused.
        map.entries.shrink_to_fit();
        for index in 0..map.entries.len() {
            let entry = map.entries[index];
            let facts = map.facts(map.value(&entry));
            map.tally(map.key(&entry).len(), facts, true);
        }
        Ok(map)
    }

    /// The serialized snapshot's length.
    pub(super) fn snapshot_len(&self) -> usize {
        varint_len(self.body_len) + self.body_len
    }

    /// The serialized length of the entries, without the length prefix.
    pub(super) fn body_len(&self) -> usize {
        self.body_len
    }

    /// The longest measured element over the element bound.
    pub(super) fn longest_oversized(&self) -> Option<usize> {
        self.oversized.keys().next_back().copied()
    }

    /// How many values are not UTF-8, in a map whose values must be.
    pub(super) fn non_utf8(&self) -> usize {
        self.non_utf8
    }

    /// The value of `key`, or `None` when it is absent. A set element's
    /// value is empty.
    pub(super) fn get(&self, key: &[u8]) -> Option<&[u8]> {
        match self.changes.get(key) {
            Some(slot) => slot.map(|span| self.arena_bytes(span)),
            None => self
                .find(key)
                .ok()
                .map(|index| self.value(&self.entries[index])),
        }
    }

    /// Copy `value` into the arena.
    pub(super) fn push(&mut self, value: &[u8]) -> Span {
        let start = self.arena.len();
        self.arena.extend_from_slice(value);
        Span {
            start,
            len: value.len(),
        }
    }

    pub(super) fn arena_len(&self) -> usize {
        self.arena.len()
    }

    /// Drop the arena bytes from `len` on; no entry refers to them.
    pub(super) fn truncate_arena(&mut self, len: usize) {
        self.arena.truncate(len);
    }

    /// Set `key` to the arena `value`, or delete it for `None`, keeping the
    /// counts and the hash index current. `hash` is the key's hash when the
    /// caller knows it. Returns what [`Self::restore`] needs to undo this.
    pub(super) fn write(
        &mut self,
        key: Box<[u8]>,
        value: Option<Span>,
        hash: Option<TlsKeyHash>,
    ) -> Replaced {
        let old = self.get(&key).map(|bytes| self.facts(bytes));
        let new = value.map(|span| self.facts(self.arena_bytes(span)));
        let hash = self.reindex(&key, old.is_some(), new.is_some(), hash);
        self.account(&key, old, false);
        self.account(&key, new, true);
        let slot = self.changes.insert(key.clone(), value);
        Replaced { key, slot, hash }
    }

    /// Undo a [`Self::write`]. Writes are undone in reverse order.
    pub(super) fn restore(&mut self, replaced: Replaced) {
        let Replaced { key, slot, hash } = replaced;
        let current = self.get(&key).map(|bytes| self.facts(bytes));
        let restored = match slot {
            Some(value) => value.map(|span| self.facts(self.arena_bytes(span))),
            None => self
                .find(&key)
                .ok()
                .map(|index| self.facts(self.value(&self.entries[index]))),
        };
        self.reindex(&key, current.is_some(), restored.is_some(), hash);
        self.account(&key, current, false);
        self.account(&key, restored, true);
        match slot {
            Some(value) => {
                self.changes.insert(key, value);
            }
            None => {
                self.changes.remove(&key);
            }
        }
    }

    /// Build the hash index of the present keys unless it is built. Fails
    /// as `TlsSet::apply_delta` does when two keys share a hash.
    // implements: META-013
    pub(super) fn ensure_index(&mut self) -> Result<(), TlsSetError> {
        if self.hashes.is_some() {
            return Ok(());
        }
        let mut index = HashMap::with_capacity(self.entries.len() + self.changes.len());
        for (origin, key, _) in self.iter() {
            let key_ref = match origin {
                Some(index) => KeyRef::Committed(index),
                None => KeyRef::Owned(key.into()),
            };
            if index
                .insert(hash_key(self.shape.key, key)?, key_ref)
                .is_some()
            {
                return Err(TlsSetError::DuplicateHash);
            }
        }
        self.hashes = Some(index);
        Ok(())
    }

    /// The present key whose hash is `hash`, once the index is built.
    pub(super) fn indexed(&self, hash: &TlsKeyHash) -> Option<Box<[u8]>> {
        match self.hashes.as_ref()?.get(hash)? {
            KeyRef::Committed(index) => Some(self.key(&self.entries[*index as usize]).into()),
            KeyRef::Owned(key) => Some(key.clone()),
        }
    }

    /// The snapshot, encoded as `TlsMap::tls_serialize` encodes it.
    // implements: META-012
    pub(super) fn encode(&self) -> Result<Vec<u8>, tls_codec::Error> {
        let mut out = Vec::with_capacity(self.snapshot_len());
        tls_codec::vlen::write_length(&mut out, self.body_len)?;
        for (_, key, value) in self.iter() {
            match self.shape.key {
                KeyKind::InboxId => {
                    inbox_id(key)?.tls_serialize(&mut out)?;
                }
                KeyKind::ComponentId => out.extend_from_slice(key),
                KeyKind::Bytes => {
                    tls_codec::vlen::write_length(&mut out, key.len())?;
                    out.extend_from_slice(key);
                }
            }
            if !self.shape.set {
                tls_codec::vlen::write_length(&mut out, value.len())?;
                out.extend_from_slice(value);
            }
        }
        debug_assert_eq!(out.len(), self.snapshot_len());
        Ok(out)
    }

    /// The present entries in key order, with the index of each committed
    /// one.
    fn iter(&self) -> Iter<'_, 'a> {
        Iter {
            map: self,
            next: 0,
            changes: self.changes.iter().peekable(),
        }
    }

    fn key(&self, entry: &Committed) -> &'a [u8] {
        let start = entry.key_start as usize;
        &self.committed[start..start + entry.key_len as usize]
    }

    fn value(&self, entry: &Committed) -> &'a [u8] {
        let start = entry.value_start as usize;
        &self.committed[start..start + entry.value_len as usize]
    }

    fn arena_bytes(&self, span: Span) -> &[u8] {
        &self.arena[span.start..span.start + span.len]
    }

    fn find(&self, key: &[u8]) -> Result<usize, usize> {
        self.entries
            .binary_search_by(|entry| self.key(entry).cmp(key))
    }

    fn facts(&self, value: &[u8]) -> Facts {
        Facts {
            len: value.len(),
            utf8: !self.shape.utf8 || std::str::from_utf8(value).is_ok(),
        }
    }

    /// The serialized length of the entry `key` with a value of
    /// `value_len` bytes.
    fn entry_len(&self, key: &[u8], value_len: usize) -> usize {
        let key_len = match self.shape.key {
            // The version varint, then the 32 bytes.
            KeyKind::InboxId => 1 + key.len(),
            KeyKind::ComponentId => key.len(),
            KeyKind::Bytes => varint_len(key.len()) + key.len(),
        };
        let value_len = if self.shape.set {
            0
        } else {
            varint_len(value_len) + value_len
        };
        key_len + value_len
    }

    /// Add the entry `key` with `value` to the counts, or take it away.
    fn account(&mut self, key: &[u8], value: Option<Facts>, add: bool) {
        let Some(value) = value else {
            return;
        };
        let len = self.entry_len(key, value.len);
        if add {
            self.body_len += len;
        } else {
            self.body_len -= len;
        }
        self.tally(key.len(), value, add);
    }

    /// Add an entry to the oversized and UTF-8 counts, or take it away.
    fn tally(&mut self, key_len: usize, value: Facts, add: bool) {
        if self.shape.measured.keys() && key_len > MAX_FIELD_ELEMENT_BYTES {
            self.oversize(key_len, add);
        }
        if self.shape.measured.values() && value.len > MAX_FIELD_ELEMENT_BYTES {
            self.oversize(value.len, add);
        }
        if !value.utf8 {
            if add {
                self.non_utf8 += 1;
            } else {
                self.non_utf8 -= 1;
            }
        }
    }

    fn oversize(&mut self, len: usize, add: bool) {
        if add {
            *self.oversized.entry(len).or_default() += 1;
        } else if let btree_map::Entry::Occupied(mut count) = self.oversized.entry(len) {
            *count.get_mut() -= 1;
            if *count.get() == 0 {
                count.remove();
            }
        }
    }

    /// Keep the hash index current when `key` appears or disappears.
    /// Returns the key's hash when it is known.
    fn reindex(
        &mut self,
        key: &[u8],
        was: bool,
        is: bool,
        hash: Option<TlsKeyHash>,
    ) -> Option<TlsKeyHash> {
        if was == is || self.hashes.is_none() {
            return hash;
        }
        let hash = match hash {
            Some(hash) => hash,
            None => match hash_key(self.shape.key, key) {
                Ok(hash) => hash,
                // A decoded key always encodes; a rebuild would report it.
                Err(_) => {
                    self.hashes = None;
                    return None;
                }
            },
        };
        let index = self.hashes.as_mut()?;
        if is {
            // Two keys with one hash: drop the index, so the next
            // `RemoveByHash` rebuilds it and fails as `TlsSet` does.
            if index.insert(hash, KeyRef::Owned(key.into())).is_some() {
                self.hashes = None;
            }
        } else {
            index.remove(&hash);
        }
        Some(hash)
    }
}

/// The present entries of a [`TrackedMap`] in key order: the committed
/// entries the changes leave, merged with the changes' present ones. Each
/// item is the committed entry's index, or `None` for a change, then its
/// key and value.
struct Iter<'m, 'a> {
    map: &'m TrackedMap<'a>,
    next: usize,
    changes: Peekable<btree_map::Iter<'m, Box<[u8]>, Option<Span>>>,
}

impl<'m> Iterator for Iter<'m, '_> {
    type Item = (Option<u32>, &'m [u8], &'m [u8]);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let committed = self.map.entries.get(self.next);
            let order = match (committed, self.changes.peek()) {
                (None, None) => return None,
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (Some(entry), Some((changed, _))) => self.map.key(entry).cmp(changed),
            };
            if let (Ordering::Less, Some(entry)) = (order, committed) {
                let index = self.next as u32;
                self.next += 1;
                return Some((Some(index), self.map.key(entry), self.map.value(entry)));
            }
            // A change replaces the committed entry with its key.
            if order == Ordering::Equal {
                self.next += 1;
            }
            let (key, slot) = self.changes.next()?;
            if let Some(span) = slot {
                return Some((None, key, self.map.arena_bytes(*span)));
            }
        }
    }
}

/// Read one key at `pos` with tls_codec's own codec, returning the range of
/// the bytes [`KeyKind`] holds.
fn read_key(
    kind: KeyKind,
    bytes: &[u8],
    pos: &mut usize,
) -> Result<(usize, usize), tls_codec::Error> {
    let mut rest = &bytes[*pos..];
    let before = rest.len();
    let range = match kind {
        KeyKind::InboxId => {
            InboxId::tls_deserialize(&mut rest)?;
            // Skip the one-byte version.
            (*pos + 1, INBOX_ID_BYTE_LEN)
        }
        KeyKind::ComponentId => {
            ComponentId::tls_deserialize(&mut rest)?;
            (*pos, before - rest.len())
        }
        KeyKind::Bytes => return read_bytes(bytes, pos),
    };
    *pos += before - rest.len();
    Ok(range)
}

/// Read one variable-length byte string at `pos`, as `VLBytes` decodes it,
/// returning the range of its bytes.
fn read_bytes(bytes: &[u8], pos: &mut usize) -> Result<(usize, usize), tls_codec::Error> {
    let mut rest = &bytes[*pos..];
    let (len, prefix) = tls_codec::vlen::read_length(&mut rest)?;
    if rest.len() < len {
        return Err(tls_codec::Error::EndOfStream);
    }
    let start = *pos + prefix;
    *pos = start + len;
    Ok((start, len))
}

/// The hash a `RemoveByHash` names `key` with: SHA-256 of its TLS encoding.
fn hash_key(kind: KeyKind, key: &[u8]) -> Result<TlsKeyHash, tls_codec::Error> {
    #[cfg(any(test, feature = "test-utils"))]
    super::counters::count_hash();
    let hash = match kind {
        KeyKind::InboxId => TlsKeyHash::of(&inbox_id(key)?),
        KeyKind::ComponentId => TlsKeyHash::of(&ComponentId::tls_deserialize_exact(key)?),
        KeyKind::Bytes => TlsKeyHash::of(&VLByteSlice(key)),
    }?;
    #[cfg(test)]
    let hash = super::counters::key_hash(hash);
    Ok(hash)
}

fn inbox_id(key: &[u8]) -> Result<InboxId, tls_codec::Error> {
    <[u8; INBOX_ID_BYTE_LEN]>::try_from(key)
        .map(InboxId::from_bytes)
        .map_err(|_| tls_codec::Error::InvalidInput)
}

/// Bytes of the QUIC varint length prefix that tls_codec writes for `len`.
pub(super) fn varint_len(len: usize) -> usize {
    match len {
        0..=0x3f => 1,
        0x40..=0x3fff => 2,
        0x4000..=0x3fff_ffff => 4,
        _ => 8,
    }
}
