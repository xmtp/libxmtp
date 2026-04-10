#![deny(missing_docs)]

//! A deterministic, sorted key-value map with TLS codec serialization.
//!
//! [`TlsMap`] maintains entries in sorted key order so that serialization is
//! byte-identical across implementations. [`TlsMapDelta`] provides an atomic
//! mutation batch (insert / update / delete) that can be serialized independently
//! and applied to a map with automatic rollback on failure.
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
//! In testing, a Vec and a BTreeMap were used to compare performance.
//! The Vec was faster even for large maps at up to 50k entries for all
//! operations except for insert and remove which were only half as fast.
//! The Vec implementation is more memory efficient and significantly
//! faster at deserialization.

use std::io::{Read, Write};

use tls_codec::{Deserialize, Serialize, Size};

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
/// use xmtp_mls_common::tls_map::TlsMap;
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
    /// use xmtp_mls_common::tls_map::TlsMap;
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
    /// use xmtp_mls_common::tls_map::TlsMap;
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
    /// use xmtp_mls_common::tls_map::TlsMap;
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
                self.entries.insert(idx, TlsMapEntry { key, value });
                Ok(())
            }
        }
    }

    /// Update an existing key's value. Returns an error if the key doesn't exist.
    ///
    /// ```
    /// use xmtp_mls_common::tls_map::TlsMap;
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
    /// use xmtp_mls_common::tls_map::TlsMap;
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
            Err(idx) => self.entries.insert(idx, TlsMapEntry { key, value }),
        }
    }

    /// Remove a key. Returns the removed value, or an error if the key doesn't exist.
    ///
    /// ```
    /// use xmtp_mls_common::tls_map::TlsMap;
    ///
    /// let mut map = TlsMap::<u8, u8>::new();
    /// map.insert(1, 10).unwrap();
    /// assert_eq!(map.remove(&1).unwrap(), 10);
    /// assert!(map.remove(&1).is_err()); // already removed
    /// ```
    #[inline]
    pub fn remove(&mut self, key: &K) -> Result<V, TlsMapError> {
        match self.entries.binary_search_by(|e| e.key.cmp(key)) {
            Ok(idx) => Ok(self.entries.remove(idx).value),
            Err(_) => Err(TlsMapError::KeyNotFound),
        }
    }

    /// Get a value by key. Returns `None` if the key is not present.
    ///
    /// ```
    /// use xmtp_mls_common::tls_map::TlsMap;
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
    /// use xmtp_mls_common::tls_map::TlsMap;
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
    /// use xmtp_mls_common::tls_map::TlsMap;
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

/// Undo action for rolling back a mutation during [`TlsMap::apply_delta`].
enum UndoAction<K, V> {
    /// Undo an insert by removing the key.
    Remove(K),
    /// Undo an update by restoring the previous value.
    Restore(K, V),
    /// Undo a delete by re-inserting the key-value pair.
    Insert(K, V),
}

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
/// use xmtp_mls_common::tls_map::TlsMapDelta;
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
    K: Ord + Eq + Clone,
{
    /// Apply a delta atomically. If any mutation fails, the map is unchanged.
    ///
    /// ```
    /// use xmtp_mls_common::tls_map::{TlsMap, TlsMapDelta};
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
    pub fn apply_delta(&mut self, delta: TlsMapDelta<K, V>) -> Result<(), TlsMapError> {
        let mut undo_stack: Vec<UndoAction<K, V>> = Vec::with_capacity(delta.mutations.len());

        for mutation in delta.mutations {
            match mutation {
                TlsMapMutation::Insert { key, value } => {
                    if let Err(e) = self.insert(key.clone(), value) {
                        self.rollback(undo_stack);
                        return Err(e);
                    }
                    undo_stack.push(UndoAction::Remove(key));
                }
                TlsMapMutation::Update { key, mut value } => match self.get_mut(&key) {
                    Some(old) => {
                        std::mem::swap(old, &mut value);
                        undo_stack.push(UndoAction::Restore(key, value));
                    }
                    None => {
                        self.rollback(undo_stack);
                        return Err(TlsMapError::KeyNotFound);
                    }
                },
                TlsMapMutation::Delete { key } => match self.remove(&key) {
                    Ok(old) => {
                        undo_stack.push(UndoAction::Insert(key, old));
                    }
                    Err(e) => {
                        self.rollback(undo_stack);
                        return Err(e);
                    }
                },
            }
        }

        Ok(())
    }

    /// Replay undo actions in reverse to restore the map to its prior state.
    fn rollback(&mut self, undo_stack: Vec<UndoAction<K, V>>) {
        for action in undo_stack.into_iter().rev() {
            match action {
                UndoAction::Remove(key) => {
                    // ignore the error as it only happens if the key is not present
                    let _ = self.remove(&key);
                }
                UndoAction::Restore(key, value) | UndoAction::Insert(key, value) => {
                    self.set(key, value);
                }
            }
        }
    }
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
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// Build a TlsMap from pairs using last-wins semantics for duplicate keys,
    /// matching real-world `HashMap::collect` behavior. Uses HashMap internally
    /// so insertion into TlsMap happens in arbitrary (non-sorted) order.
    fn build_map<K: Ord + Eq, V>(pairs: impl Iterator<Item = (K, V)>) -> TlsMap<K, V> {
        pairs.collect()
    }

    /// Generates the core property-based test suite for a given K, V type pair.
    /// Every type combination that implements the TLS traits must pass all of these.
    /// `$n` is the max number of entries to generate (must stay <= key space, e.g. 256 for u8).
    macro_rules! tls_map_tests {
        ($mod_name:ident, $K:ty, $V:ty, $n:expr, $mutate_v:expr) => {
            mod $mod_name {
                use super::*;

                proptest! {
                    #[test]
                    fn round_trip(
                        pairs in proptest::collection::vec((any::<$K>(), any::<$V>()), 0..$n)
                    ) {
                        let map = build_map(pairs.into_iter());
                        let bytes = map.tls_serialize_detached().expect("round trip serialization should succeed");
                        let deserialized =
                            TlsMap::<$K, $V>::tls_deserialize_exact(&bytes).expect("round trip deserialization should succeed");
                        prop_assert_eq!(map, deserialized);
                    }

                    #[test]
                    fn deterministic(
                        pairs in proptest::collection::vec((any::<$K>(), any::<$V>()), 0..$n)
                    ) {
                        let map = build_map(pairs.into_iter());
                        let a = map.tls_serialize_detached().unwrap();
                        let b = map.tls_serialize_detached().unwrap();
                        prop_assert_eq!(a, b);
                    }

                    #[test]
                    fn insertion_order_irrelevant(
                        pairs in proptest::collection::hash_map(
                            any::<$K>(), any::<$V>(), 2..$n
                        )
                    ) {
                        let map_a = build_map(pairs.into_iter());
                        let map_b = build_map(map_a.clone().into_iter().rev());
                        prop_assert_eq!(
                            map_a.tls_serialize_detached().unwrap(),
                            map_b.tls_serialize_detached().unwrap()
                        );
                    }

                    #[test]
                    fn keys_always_sorted(
                        pairs in proptest::collection::vec((any::<$K>(), any::<$V>()), 0..$n)
                    ) {
                        let map = build_map(pairs.into_iter());
                        let keys: Vec<&$K> = map.keys().collect();
                        for w in keys.array_windows::<2>() {
                            prop_assert!(w[0] < w[1]);
                        }
                    }

                    #[test]
                    fn get_returns_inserted(
                        pairs in proptest::collection::vec((any::<$K>(), any::<$V>()), 1..$n)
                    ) {
                        let map = build_map(pairs.clone().into_iter());
                        let expected: std::collections::HashMap<$K, $V> = pairs
                            .into_iter()
                            .collect();
                        prop_assert_eq!(map.len(), expected.len());
                        for (k, v) in &expected {
                            prop_assert_eq!(map.get(k), Some(v));
                        }
                    }

                    #[test]
                    fn insert_duplicate_fails(pairs in proptest::collection::vec((any::<$K>(), any::<$V>()), 1..$n)) {
                        let mut map = build_map(pairs.clone().into_iter());
                        for (k, v) in pairs {
                            let v2 = ($mutate_v)(v);
                            prop_assert!(matches!(
                                map.insert(k, v2),
                                Err(TlsMapError::KeyExists)
                            ));
                        }
                    }

                    #[test]
                    fn set_upsert(pairs in proptest::collection::vec((any::<$K>(), any::<$V>()), 1..$n)) {
                        let mut map = TlsMap::new();
                        let mut key_set = std::collections::BTreeSet::new();
                        for (k, v) in pairs {
                            key_set.insert(k.clone());
                            map.set(k.clone(), v.clone());
                            prop_assert_eq!(map.get(&k), Some(&v));
                            let v2 = ($mutate_v)(v);
                            map.set(k.clone(), v2.clone());
                            prop_assert_eq!(map.get(&k), Some(&v2));
                            prop_assert_eq!(map.len(), key_set.len());
                        }
                    }

                    #[test]
                    fn remove_returns_value(pairs in proptest::collection::hash_map(any::<$K>(), any::<$V>(), 2..$n)) {
                        let mut map = build_map(pairs.clone().into_iter());
                        for (k, v) in pairs {
                            prop_assert_eq!(map.remove(&k).unwrap(), v);
                        }
                        prop_assert!(map.is_empty());
                    }

                    #[test]
                    fn serialized_size_matches_trait(
                        pairs in proptest::collection::vec((any::<$K>(), any::<$V>()), 0..$n)
                    ) {
                        let map = build_map(pairs.into_iter());
                        let bytes = map.tls_serialize_detached().unwrap();
                        prop_assert_eq!(bytes.len(), map.tls_serialized_len());
                    }

                    #[test]
                    fn from_pairs_and_collect_and_set_equivalent(
                        pairs in proptest::collection::vec((any::<$K>(), any::<$V>()), 0..$n)
                    ) {
                        let map_a = TlsMap::from_pairs(pairs.clone());
                        let map_b = pairs.clone().into_iter().collect::<TlsMap<$K, $V>>();
                        let mut map_c = TlsMap::new();
                        for (k, v) in pairs {
                            map_c.set(k, v);
                        }
                        prop_assert_eq!(
                            map_a.tls_serialize_detached().unwrap(),
                            map_b.tls_serialize_detached().unwrap()
                        );
                        prop_assert_eq!(
                            map_a.tls_serialize_detached().unwrap(),
                            map_c.tls_serialize_detached().unwrap()
                        );
                    }

                    #[test]
                    fn delta_rollback_on_failure(
                        pairs in proptest::collection::vec(
                            (any::<$K>(), any::<$V>()), 1..std::cmp::min($n, 200)
                        ),
                        bad_key: $K,
                    ) {
                        let mut map = build_map(pairs.into_iter());
                        let _ = map.remove(&bad_key);
                        let snapshot = map.clone();

                        let delta = TlsMapDelta::new().update(bad_key, <$V>::default());
                        prop_assert!(map.apply_delta(delta).is_err());
                        prop_assert_eq!(map, snapshot);
                    }

                    #[test]
                    fn into_iter_sorted(
                        pairs in proptest::collection::vec((any::<$K>(), any::<$V>()), 0..$n)
                    ) {
                        let map = build_map(pairs.into_iter());
                        let collected: Vec<($K, $V)> = map.into_iter().collect();
                        for w in collected.array_windows::<2>() {
                            prop_assert!(w[0].0 < w[1].0);
                        }
                    }

                    /// Verify single-entry round-trip and wire size.
                    #[test]
                    fn single_entry_round_trip(key in any::<$K>(), value in any::<$V>()) {
                        let mut map = TlsMap::<$K, $V>::new();
                        map.insert(key, value).unwrap();
                        let bytes = map.tls_serialize_detached().unwrap();
                        prop_assert_eq!(bytes.len(), map.tls_serialized_len());
                        let deserialized = TlsMap::<$K, $V>::tls_deserialize_exact(&bytes).unwrap();
                        prop_assert_eq!(map, deserialized);
                    }

                    /// Reject bytes where two entries are serialized in descending key order.
                    #[test]
                    fn rejects_unsorted(
                        a in any::<$K>(),
                        b in any::<$K>(),
                        va in any::<$V>(),
                        vb in any::<$V>(),
                    ) {
                        if a >= b { return Ok(()); }
                        // a < b — serialize b first (wrong order)
                        let mut content = Vec::new();
                        b.tls_serialize(&mut content).unwrap();
                        va.tls_serialize(&mut content).unwrap();
                        a.tls_serialize(&mut content).unwrap();
                        vb.tls_serialize(&mut content).unwrap();

                        let mut bytes = Vec::new();
                        tls_codec::vlen::write_length(&mut bytes, content.len()).unwrap();
                        bytes.extend_from_slice(&content);

                        prop_assert!(TlsMap::<$K, $V>::tls_deserialize_exact(&bytes).is_err());
                    }

                    /// Reject bytes where the same key appears twice.
                    #[test]
                    fn rejects_duplicates(
                        key in any::<$K>(),
                        va in any::<$V>(),
                        vb in any::<$V>(),
                    ) {
                        let mut content = Vec::new();
                        key.tls_serialize(&mut content).unwrap();
                        va.tls_serialize(&mut content).unwrap();
                        key.tls_serialize(&mut content).unwrap();
                        vb.tls_serialize(&mut content).unwrap();

                        let mut bytes = Vec::new();
                        tls_codec::vlen::write_length(&mut bytes, content.len()).unwrap();
                        bytes.extend_from_slice(&content);

                        prop_assert!(TlsMap::<$K, $V>::tls_deserialize_exact(&bytes).is_err());
                    }

                    /// Mutation (Insert/Update/Delete) round-trips through serialization.
                    #[test]
                    fn mutation_round_trip(key in any::<$K>(), value in any::<$V>(), tag in 0u8..3) {
                        let mutation = match tag {
                            0 => TlsMapMutation::Insert { key, value },
                            1 => TlsMapMutation::Update { key, value },
                            _ => TlsMapMutation::Delete { key },
                        };
                        let bytes = mutation.tls_serialize_detached().unwrap();
                        let rt = TlsMapMutation::<$K, $V>::tls_deserialize_exact(&bytes).unwrap();
                        prop_assert_eq!(mutation, rt);
                    }

                    /// Insert new keys, update some existing keys, update+delete others.
                    #[test]
                    fn delta_apply_sequence(
                        existing in proptest::collection::hash_map(
                            any::<$K>(), any::<$V>(), 2..std::cmp::min($n, 50)
                        ),
                        new_entries in proptest::collection::hash_map(
                            any::<$K>(), any::<$V>(), 1..std::cmp::min($n, 50)
                        ),
                        updated_value in any::<$V>(),
                    ) {
                        let new_entries: std::collections::HashMap<_, _> = new_entries
                            .into_iter()
                            .filter(|(k, _)| !existing.contains_key(k))
                            .collect();
                        if new_entries.is_empty() { return Ok(()); }

                        // Split existing keys: first half update-only, second half update+delete
                        let existing_keys: Vec<_> = existing.keys().cloned().collect();
                        let mid = existing_keys.len() / 2;
                        let (update_only, update_and_delete) = existing_keys.split_at(mid);
                        if update_only.is_empty() || update_and_delete.is_empty() { return Ok(()); }

                        let mut map: TlsMap<$K, $V> = existing.iter()
                            .map(|(k, v)| (k.clone(), v.clone()))
                            .collect();

                        let mut delta = TlsMapDelta::new();
                        for (k, v) in &new_entries {
                            delta = delta.insert(k.clone(), v.clone());
                        }
                        for k in &existing_keys {
                            delta = delta.update(k.clone(), updated_value.clone());
                        }
                        for k in update_and_delete {
                            delta = delta.delete(k.clone());
                        }

                        map.apply_delta(delta).unwrap();
                        prop_assert_eq!(map.len(), new_entries.len() + update_only.len());
                        for (k, v) in &new_entries {
                            prop_assert_eq!(map.get(k), Some(v));
                        }
                        for k in update_only {
                            prop_assert_eq!(map.get(k), Some(&updated_value));
                        }
                        for k in update_and_delete {
                            prop_assert!(!map.contains_key(k));
                        }
                    }

                    /// Nested map with known outer key type, parameterized inner.
                    #[test]
                    fn nested_map_round_trip(
                        entries in proptest::collection::hash_map(
                            any::<u64>(),
                            proptest::collection::hash_map(
                                any::<$K>(), any::<$V>(), 0..std::cmp::min($n, 20)
                            ),
                            1..10
                        ),
                    ) {
                        let outer: TlsMap<u64, TlsMap<$K, $V>> = entries
                            .into_iter()
                            .map(|(k, inner)| (k, inner.into_iter().collect()))
                            .collect();
                        let bytes = outer.tls_serialize_detached().unwrap();
                        let deserialized = TlsMap::<u64, TlsMap<$K, $V>>::tls_deserialize_exact(&bytes).unwrap();
                        prop_assert_eq!(outer, deserialized);
                    }

                    /// Nested map with parameterized outer key, known inner key type.
                    #[test]
                    fn nested_map_reverse_round_trip(
                        entries in proptest::collection::hash_map(
                            any::<$K>(),
                            proptest::collection::hash_map(
                                any::<u64>(), any::<$V>(), 0..20
                            ),
                            1..std::cmp::min($n, 10)
                        ),
                    ) {
                        let outer: TlsMap<$K, TlsMap<u64, $V>> = entries
                            .into_iter()
                            .map(|(k, inner)| (k, inner.into_iter().collect()))
                            .collect();
                        let bytes = outer.tls_serialize_detached().unwrap();
                        let deserialized = TlsMap::<$K, TlsMap<u64, $V>>::tls_deserialize_exact(&bytes).unwrap();
                        prop_assert_eq!(outer, deserialized);
                    }

                    /// Delta (list of mutations) round-trips through serialization.
                    #[test]
                    fn delta_round_trip(
                        mutations in proptest::collection::vec(
                            (any::<$K>(), any::<$V>(), 0u8..3), 0..20
                        )
                    ) {
                        let mut delta = TlsMapDelta::new();
                        for (key, value, tag) in mutations {
                            delta = match tag {
                                0 => delta.insert(key, value),
                                1 => delta.update(key, value),
                                _ => delta.delete(key),
                            };
                        }
                        let bytes = delta.tls_serialize_detached().unwrap();
                        let rt = TlsMapDelta::<$K, $V>::tls_deserialize_exact(&bytes).unwrap();
                        prop_assert_eq!(delta, rt);
                    }
                }

                /// Verify that empty maps serialize to a single byte with value 0.
                #[test]
                fn empty_map_serializes_to_zero_length_prefix() {
                    let map = TlsMap::<$K, $V>::new();
                    assert_eq!(map.tls_serialize_detached().unwrap(), vec![0]);
                }

                #[test]
                fn debug_format() {
                    let key = <$K>::default();
                    let value = <$V>::default();
                    let entry = TlsMapEntry::<$K, $V> {
                        key: key.clone(),
                        value: value.clone(),
                    };
                    let expected = format!("{{ key: {:?}, value: {:?} }}", key, value);
                    assert_eq!(format!("{:?}", entry), expected);
                }

                #[test]
                fn get_mut_modifies_value() {
                    let mut map = TlsMap::<$K, $V>::new();
                    let key = <$K>::default();
                    let value = <$V>::default();
                    map.insert(key.clone(), value).unwrap();
                    let v = map.get_mut(&key).unwrap();
                    *v = ($mutate_v)(v.clone());
                    assert_ne!(map.get(&key), Some(&<$V>::default()));
                }

                #[test]
                fn iter_yields_all_pairs() {
                    let mut map = TlsMap::<$K, $V>::new();
                    map.set(<$K>::default(), <$V>::default());
                    let pairs: Vec<_> = map.iter().collect();
                    assert_eq!(pairs.len(), 1);
                    assert_eq!(pairs[0], (&<$K>::default(), &<$V>::default()));
                }

                #[test]
                fn values_yields_all_values() {
                    let mut map = TlsMap::<$K, $V>::new();
                    map.set(<$K>::default(), <$V>::default());
                    let vals: Vec<_> = map.values().collect();
                    assert_eq!(vals, vec![&<$V>::default()]);
                }

                #[test]
                fn default_creates_empty() {
                    let map = TlsMap::<$K, $V>::default();
                    assert!(map.is_empty());
                    let delta = TlsMapDelta::<$K, $V>::default();
                    assert!(delta.mutations.is_empty());
                }

                #[test]
                fn rejects_invalid_mutation_tag() {
                    let mut bytes = Vec::new();
                    let tag_byte = 3u8; // invalid tag
                    let key = <$K>::default();
                    let value = <$V>::default();
                    let content_len = 1 + key.tls_serialized_len() + value.tls_serialized_len();
                    tls_codec::vlen::write_length(&mut bytes, content_len).unwrap();
                    tag_byte.tls_serialize(&mut bytes).unwrap();
                    key.tls_serialize(&mut bytes).unwrap();
                    value.tls_serialize(&mut bytes).unwrap();
                    assert!(TlsMapDelta::<$K, $V>::tls_deserialize_exact(&bytes).is_err());
                }

                /// Deserializing a map with descending keys must fail.
                #[test]
                fn rejects_unsorted_deterministic() {
                    // Serialize a valid 2-entry map, then re-encode entries in reverse order
                    // Build two entries: key_hi > key_lo, serialize hi first (wrong order)
                    let key_lo = <$K>::default();
                    let key_hi = {
                        // Serialize default key, bump last byte to get a larger key
                        let mut kb = key_lo.tls_serialize_detached().unwrap();
                        *kb.last_mut().unwrap() = kb.last().unwrap().wrapping_add(1);
                        kb
                    };
                    let val = <$V>::default().tls_serialize_detached().unwrap();
                    // content = [key_hi, val, key_lo, val] — descending order
                    let mut content = Vec::new();
                    content.extend_from_slice(&key_hi);
                    content.extend_from_slice(&val);
                    content.extend_from_slice(&key_lo.tls_serialize_detached().unwrap());
                    content.extend_from_slice(&val);
                    let mut bytes = Vec::new();
                    tls_codec::vlen::write_length(&mut bytes, content.len()).unwrap();
                    bytes.extend_from_slice(&content);
                    let result = TlsMap::<$K, $V>::tls_deserialize_exact(&bytes);
                    assert!(result.is_err(), "should reject unsorted entries");
                }

                /// Deserializing a map with duplicate keys must fail.
                #[test]
                fn rejects_duplicates_deterministic() {
                    let key = <$K>::default();
                    let v1 = <$V>::default();
                    let v2 = ($mutate_v)(<$V>::default());
                    let mut content = Vec::new();
                    key.tls_serialize(&mut content).unwrap();
                    v1.tls_serialize(&mut content).unwrap();
                    key.tls_serialize(&mut content).unwrap();
                    v2.tls_serialize(&mut content).unwrap();
                    let mut bytes = Vec::new();
                    tls_codec::vlen::write_length(&mut bytes, content.len()).unwrap();
                    bytes.extend_from_slice(&content);
                    let result = TlsMap::<$K, $V>::tls_deserialize_exact(&bytes);
                    assert!(result.is_err(), "should reject duplicate keys");
                }

                /// Appending extra bytes to a valid map must fail with tls_deserialize_exact.
                #[test]
                fn rejects_trailing_bytes() {
                    let mut map = TlsMap::<$K, $V>::new();
                    map.set(<$K>::default(), <$V>::default());
                    let mut bytes = map.tls_serialize_detached().unwrap();
                    bytes.push(0xFF);
                    assert!(TlsMap::<$K, $V>::tls_deserialize_exact(&bytes).is_err());
                }
            }
        };
    }

    // u8 keys: 1000 entries across 256 key space guarantees heavy collisions
    tls_map_tests!(u8_u8, u8, u8, 1000, |v: u8| v.wrapping_add(1));
    tls_map_tests!(u8_u16, u8, u16, 1000, |v: u16| v.wrapping_add(1));
    tls_map_tests!(u8_u32, u8, u32, 1000, |v: u32| v.wrapping_add(1));
    tls_map_tests!(u8_u64, u8, u64, 1000, |v: u64| v.wrapping_add(1));
    tls_map_tests!(u16_u8, u16, u8, 1000, |v: u8| v.wrapping_add(1));
    tls_map_tests!(u16_u16, u16, u16, 1000, |v: u16| v.wrapping_add(1));
    tls_map_tests!(u16_u32, u16, u32, 1000, |v: u32| v.wrapping_add(1));
    tls_map_tests!(u16_u64, u16, u64, 1000, |v: u64| v.wrapping_add(1));
    tls_map_tests!(u32_u8, u32, u8, 1000, |v: u8| v.wrapping_add(1));
    tls_map_tests!(u32_u16, u32, u16, 1000, |v: u16| v.wrapping_add(1));
    tls_map_tests!(u32_u32, u32, u32, 1000, |v: u32| v.wrapping_add(1));
    tls_map_tests!(u32_u64, u32, u64, 1000, |v: u64| v.wrapping_add(1));
    tls_map_tests!(u64_u8, u64, u8, 1000, |v: u8| v.wrapping_add(1));
    tls_map_tests!(u64_u16, u64, u16, 1000, |v: u16| v.wrapping_add(1));
    tls_map_tests!(u64_u32, u64, u32, 1000, |v: u32| v.wrapping_add(1));
    tls_map_tests!(u64_u64, u64, u64, 1000, |v: u64| v.wrapping_add(1));
    tls_map_tests!(u16_32_bytes, u16, [u8; 32], 1000, |v: [u8; 32]| {
        let mut v = v;
        v[0] = v[0].wrapping_add(1);
        v
    });
    tls_map_tests!(u64_vec, u64, Vec<u8>, 1000, |v: Vec<u8>| {
        let mut v = v;
        v.push(42);
        v
    });
    tls_map_tests!(_32_bytes_vec, [u8; 32], Vec<u8>, 1000, |v: Vec<u8>| {
        let mut v = v;
        v.push(1);
        v
    });
    tls_map_tests!(vec_vec, Vec<u8>, Vec<u8>, 1000, |v: Vec<u8>| {
        let mut v = v;
        v.push(2);
        v
    });
}
