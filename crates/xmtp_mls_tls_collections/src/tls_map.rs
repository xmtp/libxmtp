#![deny(missing_docs)]

//! A deterministic, sorted key-value map with TLS codec serialization.
//!
//! [`TlsMap`] maintains entries in sorted key order so that serialization is
//! byte-identical across implementations. [`TlsMapDelta`] provides an atomic
//! mutation batch (insert / update / delete) that can be serialized independently
//! and applied to a map, which a failed delta leaves unchanged.
//!
//! # Complexity
//!
//! Backed by a sorted `Vec`, so lookup is **O(log n)** via binary search, while
//! insert, update, and remove are **O(n)** due to element shifting. This is a
//! deliberate trade-off for deterministic serialization and small map sizes
//! typical in MLS group state. Do not use this as a general-purpose map for
//! large datasets — use [`std::collections::BTreeMap`] or [`std::collections::HashMap`]
//! instead.
//!
//! [`TlsMap::apply_delta`] does not shift entries per mutation: it collects a
//! delta's changes and merges them into the entries once, so a delta of `m`
//! mutations to a map of `n` entries costs **O(m log(n + m) + n)**.
//!
//! In testing, a Vec and a BTreeMap were used to compare performance.
//! The Vec was faster even for large maps at up to 50k entries for all
//! operations except for insert and remove which were only half as fast.
//! The Vec implementation is more memory efficient and significantly
//! faster at deserialization.

use std::collections::{BTreeMap, btree_map::Entry};
use std::io::{Read, Write};

use tls_codec::{Deserialize, Serialize, Size};

#[cfg(test)]
thread_local! {
    /// Entries this thread wrote to a new position in an entry vector.
    static ENTRY_MOVES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Count `moved` entries written to a new position. Only test builds keep
/// the count; `count_entry_moves` reads it.
#[inline]
fn count_moves(moved: usize) {
    #[cfg(test)]
    ENTRY_MOVES.set(ENTRY_MOVES.get() + moved as u64);
    #[cfg(not(test))]
    let _ = moved;
}

/// Run `f` and return how many entries it wrote to a new position.
#[cfg(test)]
pub(crate) fn count_entry_moves<R>(f: impl FnOnce() -> R) -> (R, u64) {
    let before = ENTRY_MOVES.get();
    let result = f();
    (result, ENTRY_MOVES.get() - before)
}

/// Error type for [`TlsMap`] operations.
#[derive(Debug, thiserror::Error)]
pub enum TlsMapError {
    /// Attempted to insert a key that already exists.
    #[error("key already exists")]
    KeyExists,
    /// Attempted to update or remove a key that does not exist.
    #[error("key not found")]
    KeyNotFound,
    /// A TLS codec serialization or deserialization error.
    #[error("tls codec error: {0}")]
    Codec(#[from] tls_codec::Error),
}

/// A single key-value entry in a [`TlsMap`].
///
/// Wire format: `K(tls) || V(tls)` — key followed by value with no delimiter.
#[derive(Clone, PartialEq, Eq)]
pub struct TlsMapEntry<K, V> {
    /// The entry's key.
    pub key: K,
    /// The entry's value.
    pub value: V,
}

impl<K: std::fmt::Debug, V: std::fmt::Debug> std::fmt::Debug for TlsMapEntry<K, V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{{ key: {:?}, value: {:?} }}", self.key, self.value)
    }
}

impl<K: Size, V: Size> Size for TlsMapEntry<K, V> {
    #[inline]
    fn tls_serialized_len(&self) -> usize {
        self.key.tls_serialized_len() + self.value.tls_serialized_len()
    }
}

impl<K: Serialize, V: Serialize> Serialize for TlsMapEntry<K, V> {
    #[inline]
    fn tls_serialize<W: Write>(&self, writer: &mut W) -> Result<usize, tls_codec::Error> {
        let key_size = self.key.tls_serialize(writer)?;
        let value_size = self.value.tls_serialize(writer)?;
        Ok(key_size + value_size)
    }
}

impl<K: Deserialize + Size, V: Deserialize + Size> Deserialize for TlsMapEntry<K, V> {
    #[inline]
    fn tls_deserialize<R: Read>(bytes: &mut R) -> Result<Self, tls_codec::Error>
    where
        Self: Sized,
    {
        let key = K::tls_deserialize(bytes)?;
        let value = V::tls_deserialize(bytes)?;
        Ok(Self { key, value })
    }
}

/// A sorted key-value map with deterministic TLS codec serialization.
///
/// Entries are maintained in sorted order by key, ensuring byte-identical
/// serialization across all implementations. Keys must be unique.
///
/// Wire format: `vlen(total_byte_length) || entry[0] || entry[1] || ...`
/// where each entry is `K(tls) || V(tls)` and `vlen` is the QUIC
/// variable-length encoding (RFC 9000 §16).
///
/// # Examples
///
/// ```
/// use xmtp_mls_tls_collections::tls_map::TlsMap;
/// use tls_codec::{Serialize, Deserialize};
///
/// let mut map = TlsMap::<u16, u16>::new();
/// map.insert(3, 30).unwrap();
/// map.insert(1, 10).unwrap();
/// map.insert(2, 20).unwrap();
///
/// // Serialization is deterministic regardless of insertion order
/// let bytes = map.tls_serialize_detached().unwrap();
/// let deserialized = TlsMap::<u16, u16>::tls_deserialize_exact(&bytes).unwrap();
/// assert_eq!(map, deserialized);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TlsMap<K, V> {
    entries: Vec<TlsMapEntry<K, V>>,
}

impl<K, V> Default for TlsMap<K, V> {
    #[inline]
    fn default() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
}

impl<K, V> TlsMap<K, V>
where
    K: Ord + Eq,
{
    /// Create an empty map.
    ///
    /// ```
    /// use xmtp_mls_tls_collections::tls_map::TlsMap;
    ///
    /// let map = TlsMap::<u8, u16>::new();
    /// assert!(map.is_empty());
    /// ```
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a map from an iterator of key-value pairs.
    ///
    /// Entries are sorted by key. If duplicate keys exist, the last value wins.
    ///
    /// ```
    /// use xmtp_mls_tls_collections::tls_map::TlsMap;
    ///
    /// let map = TlsMap::from_pairs([(2, "b"), (1, "a"), (2, "c")]);
    /// assert_eq!(map.get(&1), Some(&"a"));
    /// assert_eq!(map.get(&2), Some(&"c")); // last value wins
    /// assert_eq!(map.len(), 2);
    /// ```
    pub fn from_pairs(iter: impl IntoIterator<Item = (K, V)>) -> Self {
        let entries = iter
            .into_iter()
            .collect::<std::collections::BTreeMap<K, V>>()
            .into_iter()
            .map(|(key, value)| TlsMapEntry { key, value })
            .collect();
        Self { entries }
    }

    /// Insert a key-value pair. Returns an error if the key already exists.
    ///
    /// ```
    /// use xmtp_mls_tls_collections::tls_map::TlsMap;
    ///
    /// let mut map = TlsMap::<u8, u8>::new();
    /// assert!(map.insert(1, 10).is_ok());
    /// assert!(map.insert(1, 20).is_err()); // duplicate key
    /// ```
    #[inline]
    pub fn insert(&mut self, key: K, value: V) -> Result<(), TlsMapError> {
        match self.entries.binary_search_by(|e| e.key.cmp(&key)) {
            Ok(_) => Err(TlsMapError::KeyExists),
            Err(idx) => {
                count_moves(self.entries.len() - idx + 1);
                self.entries.insert(idx, TlsMapEntry { key, value });
                Ok(())
            }
        }
    }

    /// Update an existing key's value. Returns an error if the key doesn't exist.
    ///
    /// ```
    /// use xmtp_mls_tls_collections::tls_map::TlsMap;
    ///
    /// let mut map = TlsMap::<u8, u8>::new();
    /// map.insert(1, 10).unwrap();
    /// map.update(1, 20).unwrap();
    /// assert_eq!(map.get(&1), Some(&20));
    /// assert!(map.update(99, 0).is_err()); // key not found
    /// ```
    #[inline]
    pub fn update(&mut self, key: K, value: V) -> Result<(), TlsMapError> {
        match self.entries.binary_search_by(|e| e.key.cmp(&key)) {
            Ok(idx) => {
                self.entries[idx].value = value;
                Ok(())
            }
            Err(_) => Err(TlsMapError::KeyNotFound),
        }
    }

    /// Insert or update a key-value pair. Always succeeds.
    ///
    /// ```
    /// use xmtp_mls_tls_collections::tls_map::TlsMap;
    ///
    /// let mut map = TlsMap::<u8, u8>::new();
    /// map.set(1, 10);
    /// map.set(1, 20); // overwrites
    /// assert_eq!(map.get(&1), Some(&20));
    /// assert_eq!(map.len(), 1);
    /// ```
    #[inline]
    pub fn set(&mut self, key: K, value: V) {
        match self.entries.binary_search_by(|e| e.key.cmp(&key)) {
            Ok(idx) => self.entries[idx].value = value,
            Err(idx) => {
                count_moves(self.entries.len() - idx + 1);
                self.entries.insert(idx, TlsMapEntry { key, value });
            }
        }
    }

    /// Remove a key. Returns the removed value, or an error if the key doesn't exist.
    ///
    /// ```
    /// use xmtp_mls_tls_collections::tls_map::TlsMap;
    ///
    /// let mut map = TlsMap::<u8, u8>::new();
    /// map.insert(1, 10).unwrap();
    /// assert_eq!(map.remove(&1).unwrap(), 10);
    /// assert!(map.remove(&1).is_err()); // already removed
    /// ```
    #[inline]
    pub fn remove(&mut self, key: &K) -> Result<V, TlsMapError> {
        match self.entries.binary_search_by(|e| e.key.cmp(key)) {
            Ok(idx) => {
                count_moves(self.entries.len() - idx - 1);
                Ok(self.entries.remove(idx).value)
            }
            Err(_) => Err(TlsMapError::KeyNotFound),
        }
    }

    /// Get a value by key. Returns `None` if the key is not present.
    ///
    /// ```
    /// use xmtp_mls_tls_collections::tls_map::TlsMap;
    ///
    /// let mut map = TlsMap::<u8, u8>::new();
    /// map.insert(1, 10).unwrap();
    /// assert_eq!(map.get(&1), Some(&10));
    /// assert_eq!(map.get(&2), None);
    /// ```
    #[inline]
    pub fn get(&self, key: &K) -> Option<&V> {
        self.entries
            .binary_search_by(|e| e.key.cmp(key))
            .ok()
            .map(|idx| &self.entries[idx].value)
    }

    /// Get a mutable reference to a value by key. Returns `None` if the key is not present.
    ///
    /// ```
    /// use xmtp_mls_tls_collections::tls_map::TlsMap;
    ///
    /// let mut map = TlsMap::<u8, u8>::new();
    /// map.insert(1, 10).unwrap();
    /// *map.get_mut(&1).unwrap() = 20;
    /// assert_eq!(map.get(&1), Some(&20));
    /// ```
    #[inline]
    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        self.entries
            .binary_search_by(|e| e.key.cmp(key))
            .ok()
            .map(|idx| &mut self.entries[idx].value)
    }

    /// Check if a key exists in the map.
    #[inline]
    pub fn contains_key(&self, key: &K) -> bool {
        self.entries.binary_search_by(|e| e.key.cmp(key)).is_ok()
    }

    /// Returns the number of entries in the map.
    #[inline]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns true if the map contains no entries.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterate over key-value pairs in key-sorted order.
    ///
    /// ```
    /// use xmtp_mls_tls_collections::tls_map::TlsMap;
    ///
    /// let map = TlsMap::from_pairs([(2, "b"), (1, "a")]);
    /// let pairs: Vec<_> = map.iter().collect();
    /// assert_eq!(pairs, vec![(&1, &"a"), (&2, &"b")]);
    /// ```
    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.entries.iter().map(|e| (&e.key, &e.value))
    }

    /// Iterate over keys in sorted order.
    #[inline]
    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.entries.iter().map(|e| &e.key)
    }

    /// Iterate over values in key-sorted order.
    #[inline]
    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.entries.iter().map(|e| &e.value)
    }
}

// -- TLS codec for TlsMap --
// Delegates to Vec<TlsMapEntry<K, V>> for the QUIC variable-length wire format,
// then validates sorted + unique keys on deserialization.

impl<K: Size, V: Size> Size for TlsMap<K, V> {
    #[inline]
    fn tls_serialized_len(&self) -> usize {
        self.entries.tls_serialized_len()
    }
}

impl<K: Serialize + Size + std::fmt::Debug, V: Serialize + Size + std::fmt::Debug> Serialize
    for TlsMap<K, V>
{
    #[inline]
    fn tls_serialize<W: Write>(&self, writer: &mut W) -> Result<usize, tls_codec::Error> {
        self.entries.tls_serialize(writer)
    }
}

impl<K, V> Deserialize for TlsMap<K, V>
where
    K: Deserialize + Size + Ord + Eq,
    V: Deserialize + Size,
{
    // implements: META-012
    fn tls_deserialize<R: Read>(bytes: &mut R) -> Result<Self, tls_codec::Error>
    where
        Self: Sized,
    {
        let entries = Vec::<TlsMapEntry<K, V>>::tls_deserialize(bytes)?;

        // Verify sorted and unique
        for [l, r] in entries.array_windows::<2>() {
            if l.key >= r.key {
                return Err(tls_codec::Error::DecodingError(
                    "TlsMap entries not sorted or contain duplicates".into(),
                ));
            }
        }

        Ok(Self { entries })
    }
}

// -- Delta operations --

/// A single mutation to apply to a [`TlsMap`].
///
/// Wire format: `u8(tag) || K(tls) [|| V(tls)]` where tag 0 = Insert, 1 = Update, 2 = Delete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TlsMapMutation<K, V> {
    /// Insert a new key-value pair. Fails if the key already exists.
    Insert {
        /// The key to insert.
        key: K,
        /// The value to associate with the key.
        value: V,
    },
    /// Update an existing key's value. Fails if the key doesn't exist.
    Update {
        /// The key to update.
        key: K,
        /// The new value.
        value: V,
    },
    /// Delete a key. Fails if the key doesn't exist.
    Delete {
        /// The key to delete.
        key: K,
    },
}

/// A list of mutations to apply atomically to a [`TlsMap`].
///
/// Built with a fluent API and applied via [`TlsMap::apply_delta`].
///
/// ```
/// use xmtp_mls_tls_collections::tls_map::TlsMapDelta;
///
/// let delta = TlsMapDelta::<u8, u8>::new()
///     .insert(1, 10)
///     .update(2, 20)
///     .delete(3);
/// assert_eq!(delta.mutations.len(), 3);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TlsMapDelta<K, V> {
    /// The ordered list of mutations to apply.
    pub mutations: Vec<TlsMapMutation<K, V>>,
}

impl<K, V> TlsMapDelta<K, V> {
    /// Create an empty delta with no mutations.
    #[inline]
    pub fn new() -> Self {
        Self {
            mutations: Vec::new(),
        }
    }

    /// Append an insert mutation. Consumes and returns `self` for chaining.
    #[inline]
    pub fn insert(mut self, key: K, value: V) -> Self {
        self.mutations.push(TlsMapMutation::Insert { key, value });
        self
    }

    /// Append an update mutation. Consumes and returns `self` for chaining.
    #[inline]
    pub fn update(mut self, key: K, value: V) -> Self {
        self.mutations.push(TlsMapMutation::Update { key, value });
        self
    }

    /// Append a delete mutation. Consumes and returns `self` for chaining.
    #[inline]
    pub fn delete(mut self, key: K) -> Self {
        self.mutations.push(TlsMapMutation::Delete { key });
        self
    }
}

impl<K, V> Default for TlsMapDelta<K, V> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<K, V> TlsMap<K, V>
where
    K: Ord + Eq,
{
    /// Apply a delta atomically. If any mutation fails, the map is unchanged.
    ///
    /// Mutations apply in order, each to the result of the ones before it,
    /// and the first one that fails decides the error: an insert of a
    /// present key is [`TlsMapError::KeyExists`], and an update or delete of
    /// an absent key is [`TlsMapError::KeyNotFound`]. The changed keys
    /// collect in an overlay that is merged into the entries once, after
    /// every mutation succeeds.
    ///
    /// ```
    /// use xmtp_mls_tls_collections::tls_map::{TlsMap, TlsMapDelta};
    ///
    /// let mut map = TlsMap::<u8, u8>::new();
    /// map.insert(1, 10).unwrap();
    ///
    /// let delta = TlsMapDelta::new()
    ///     .insert(2, 20)
    ///     .update(1, 15)
    ///     .delete(1);
    ///
    /// map.apply_delta(delta).unwrap();
    /// assert_eq!(map.get(&2), Some(&20));
    /// assert!(!map.contains_key(&1));
    /// ```
    // implements: META-013
    pub fn apply_delta(&mut self, delta: TlsMapDelta<K, V>) -> Result<(), TlsMapError> {
        let mut changed = BTreeMap::<K, Change<V>>::new();
        for mutation in delta.mutations {
            let (key, value, must_exist) = match mutation {
                TlsMapMutation::Insert { key, value } => (key, Some(value), false),
                TlsMapMutation::Update { key, value } => (key, Some(value), true),
                TlsMapMutation::Delete { key } => (key, None, true),
            };
            let present = changed
                .get(&key)
                .map_or_else(|| self.contains_key(&key), |change| change.value.is_some());
            if present != must_exist {
                return Err(if present {
                    TlsMapError::KeyExists
                } else {
                    TlsMapError::KeyNotFound
                });
            }
            if must_exist {
                // An update or delete keeps the key already stored.
                match changed.entry(key) {
                    Entry::Occupied(mut change) => change.get_mut().value = value,
                    Entry::Vacant(change) => {
                        change.insert(Change {
                            value,
                            inserted: false,
                        });
                    }
                }
            } else {
                // An insert stores its own key, also in place of an equal key
                // that the delta deleted.
                changed.remove(&key);
                changed.insert(
                    key,
                    Change {
                        value,
                        inserted: true,
                    },
                );
            }
        }
        self.merge(changed);
        Ok(())
    }

    /// Merge `changed` into the entries in one pass: a changed key's value
    /// replaces its entry, and a deleted key drops it.
    fn merge(&mut self, changed: BTreeMap<K, Change<V>>) {
        if changed.is_empty() {
            return;
        }
        let mut entries = std::mem::take(&mut self.entries).into_iter().peekable();
        let mut merged = Vec::with_capacity(entries.len() + changed.len());
        for (key, change) in changed {
            // Keep the entries before the changed key.
            while let Some(entry) = entries.next_if(|entry| entry.key < key) {
                merged.push(entry);
            }
            // Take the entry that the change replaces or deletes.
            let stored = entries.next_if(|entry| entry.key == key);
            if let Some(value) = change.value {
                let key = match stored {
                    Some(stored) if !change.inserted => stored.key,
                    _ => key,
                };
                merged.push(TlsMapEntry { key, value });
            }
        }
        merged.extend(entries);
        count_moves(merged.len());
        self.entries = merged;
    }
}

/// One key's change in [`TlsMap::apply_delta`], after the mutations so far.
struct Change<V> {
    /// The key's value; `None` once deleted.
    value: Option<V>,
    /// Whether the delta inserted the key. The entry then stores the delta's
    /// key, as a sequential insert does; otherwise it keeps its stored key,
    /// as a sequential update does.
    inserted: bool,
}

// -- TLS codec for delta types --

// Mutation tag: 0 = Insert, 1 = Update, 2 = Delete
impl<K: Size, V: Size> Size for TlsMapMutation<K, V> {
    fn tls_serialized_len(&self) -> usize {
        1 + match self {
            Self::Insert { key, value } | Self::Update { key, value } => {
                key.tls_serialized_len() + value.tls_serialized_len()
            }
            Self::Delete { key } => key.tls_serialized_len(),
        }
    }
}

impl<K: Serialize + Size, V: Serialize + Size> Serialize for TlsMapMutation<K, V> {
    fn tls_serialize<W: Write>(&self, writer: &mut W) -> Result<usize, tls_codec::Error> {
        match self {
            Self::Insert { key, value } => {
                let mut written = 0u8.tls_serialize(writer)?;
                written += key.tls_serialize(writer)?;
                written += value.tls_serialize(writer)?;
                Ok(written)
            }
            Self::Update { key, value } => {
                let mut written = 1u8.tls_serialize(writer)?;
                written += key.tls_serialize(writer)?;
                written += value.tls_serialize(writer)?;
                Ok(written)
            }
            Self::Delete { key } => {
                let mut written = 2u8.tls_serialize(writer)?;
                written += key.tls_serialize(writer)?;
                Ok(written)
            }
        }
    }
}

impl<K, V> Deserialize for TlsMapMutation<K, V>
where
    K: Deserialize + Size,
    V: Deserialize + Size,
{
    fn tls_deserialize<R: Read>(bytes: &mut R) -> Result<Self, tls_codec::Error>
    where
        Self: Sized,
    {
        let tag = u8::tls_deserialize(bytes)?;
        match tag {
            0 => {
                let key = K::tls_deserialize(bytes)?;
                let value = V::tls_deserialize(bytes)?;
                Ok(Self::Insert { key, value })
            }
            1 => {
                let key = K::tls_deserialize(bytes)?;
                let value = V::tls_deserialize(bytes)?;
                Ok(Self::Update { key, value })
            }
            2 => {
                let key = K::tls_deserialize(bytes)?;
                Ok(Self::Delete { key })
            }
            _ => Err(tls_codec::Error::DecodingError(format!(
                "unknown TlsMapMutation tag: {tag}"
            ))),
        }
    }
}

impl<K: Size, V: Size> Size for TlsMapDelta<K, V> {
    #[inline]
    fn tls_serialized_len(&self) -> usize {
        self.mutations.tls_serialized_len()
    }
}

impl<K: Serialize + Size + std::fmt::Debug, V: Serialize + Size + std::fmt::Debug> Serialize
    for TlsMapDelta<K, V>
{
    #[inline]
    fn tls_serialize<W: Write>(&self, writer: &mut W) -> Result<usize, tls_codec::Error> {
        self.mutations.tls_serialize(writer)
    }
}

impl<K, V> Deserialize for TlsMapDelta<K, V>
where
    K: Deserialize + Size,
    V: Deserialize + Size,
{
    fn tls_deserialize<R: Read>(bytes: &mut R) -> Result<Self, tls_codec::Error>
    where
        Self: Sized,
    {
        let mutations = Vec::<TlsMapMutation<K, V>>::tls_deserialize(bytes)?;
        Ok(Self { mutations })
    }
}

// -- IntoIterator --

impl<K, V> IntoIterator for TlsMap<K, V> {
    type Item = (K, V);
    type IntoIter =
        std::iter::Map<std::vec::IntoIter<TlsMapEntry<K, V>>, fn(TlsMapEntry<K, V>) -> (K, V)>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.entries.into_iter().map(|e| (e.key, e.value))
    }
}

impl<K: Ord + Eq, V> FromIterator<(K, V)> for TlsMap<K, V> {
    #[inline]
    fn from_iter<T: IntoIterator<Item = (K, V)>>(iter: T) -> Self {
        Self::from_pairs(iter)
    }
}

#[cfg(test)]
mod tests;
