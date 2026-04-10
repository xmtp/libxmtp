#![deny(missing_docs)]

//! A deterministic, sorted set with TLS codec serialization.
//!
//! [`TlsSet`] is backed by a [`TlsMap<K, ()>`](crate::tls_map::TlsMap), inheriting
//! deterministic serialization, O(log n) lookup, and O(n) insert/remove.
//!
//! Since `()` serializes as zero bytes in TLS codec, the wire format is
//! effectively `vlen(total_byte_length) || key[0] || key[1] || ...`.

use std::io::{Read, Write};

use tls_codec::{Deserialize, Serialize, Size};

use crate::tls_map::{TlsMap, TlsMapDelta, TlsMapError, TlsMapMutation};

/// A SHA-256 hash of a key's TLS-serialized form, used as the lookup token
/// for [`TlsSetMutation::RemoveByHash`].
///
/// This is a newtype around `[u8; 32]` to prevent accidental mixing with
/// other 32-byte SHA-256 hashes elsewhere in the codebase (commit hashes,
/// installation IDs, etc.).
///
/// The only safe way to construct one is via [`TlsKeyHash::of`], which
/// guarantees the hash always uses the canonical TLS serialization of the
/// key. Raw byte construction is reserved for wire-format deserialization.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct TlsKeyHash([u8; 32]);

impl TlsKeyHash {
    /// Compute the hash of a key by SHA-256-ing its TLS-serialized form.
    /// This is the only public way to construct a `TlsKeyHash`, ensuring
    /// every hash on the wire was produced by the canonical encoding.
    #[inline]
    pub fn of<K: Serialize + Size>(key: &K) -> Result<Self, tls_codec::Error> {
        let bytes = key.tls_serialize_detached()?;
        Ok(Self(xmtp_common::sha256_array(&bytes)))
    }

    /// Construct from raw bytes. Used by TLS deserialization to reconstruct
    /// a hash that was produced by an earlier `TlsKeyHash::of` on the sender.
    #[inline]
    pub(crate) const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Borrow the underlying bytes.
    #[inline]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl std::fmt::Debug for TlsKeyHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TlsKeyHash(")?;
        for byte in &self.0 {
            write!(f, "{byte:02x}")?;
        }
        f.write_str(")")
    }
}

/// Error type for [`TlsSet`] operations.
#[derive(Debug, thiserror::Error)]
pub enum TlsSetError {
    /// Two or more keys in the set produce the same SHA-256 digest.
    ///
    /// Returned by [`TlsSet::apply_delta`] when building the hash index for
    /// `RemoveByHash` lookups. SHA-256 collisions are cryptographically
    /// infeasible to find by chance, so in practice this signals either:
    /// - A bug in `Serialize` for `K` producing identical bytes for distinct
    ///   logically-different keys (e.g., a non-deterministic encoding), or
    /// - A deliberate adversarial attempt to seed the set with crafted entries.
    ///
    /// We surface it as a defense-in-depth guarantee: `RemoveByHash` resolves
    /// to exactly one key, never silently picking one of several candidates.
    #[error("duplicate hash in set")]
    DuplicateHash,
    /// An underlying [`TlsMap`] error.
    #[error(transparent)]
    Map(#[from] TlsMapError),
    /// A TLS codec serialization error (from hashing a key).
    #[error("tls codec error: {0}")]
    Codec(#[from] tls_codec::Error),
}

/// A sorted set with deterministic TLS codec serialization.
///
/// # Examples
///
/// ```
/// use xmtp_mls_common::tls_set::TlsSet;
/// use tls_codec::{Serialize, Deserialize};
///
/// let mut set = TlsSet::<u16>::new();
/// set.insert(3).unwrap();
/// set.insert(1).unwrap();
/// set.insert(2).unwrap();
///
/// let bytes = set.tls_serialize_detached().unwrap();
/// let deserialized = TlsSet::<u16>::tls_deserialize_exact(&bytes).unwrap();
/// assert_eq!(set, deserialized);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TlsSet<K> {
    inner: TlsMap<K, ()>,
}

impl<K> Default for TlsSet<K> {
    #[inline]
    fn default() -> Self {
        Self {
            inner: TlsMap::default(),
        }
    }
}

impl<K: Ord + Eq> TlsSet<K> {
    /// Create an empty set.
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a set from an iterator of keys. Duplicates are silently ignored.
    ///
    /// ```
    /// use xmtp_mls_common::tls_set::TlsSet;
    ///
    /// let set = TlsSet::from_keys([3, 1, 2, 1]);
    /// assert_eq!(set.len(), 3);
    /// ```
    #[inline]
    pub fn from_keys(iter: impl IntoIterator<Item = K>) -> Self {
        Self {
            inner: TlsMap::from_pairs(iter.into_iter().map(|k| (k, ()))),
        }
    }

    /// Insert a key. Returns an error if the key already exists.
    ///
    /// ```
    /// use xmtp_mls_common::tls_set::TlsSet;
    ///
    /// let mut set = TlsSet::<u8>::new();
    /// assert!(set.insert(1).is_ok());
    /// assert!(set.insert(1).is_err()); // duplicate
    /// ```
    #[inline]
    pub fn insert(&mut self, key: K) -> Result<(), TlsSetError> {
        Ok(self.inner.insert(key, ())?)
    }

    /// Remove a key. Returns an error if the key doesn't exist.
    ///
    /// ```
    /// use xmtp_mls_common::tls_set::TlsSet;
    ///
    /// let mut set = TlsSet::<u8>::new();
    /// set.insert(1).unwrap();
    /// assert!(set.remove(&1).is_ok());
    /// assert!(set.remove(&1).is_err()); // already removed
    /// ```
    #[inline]
    pub fn remove(&mut self, key: &K) -> Result<(), TlsSetError> {
        self.inner.remove(key).map(|_| ())?;
        Ok(())
    }

    /// Returns true if the set contains the key.
    #[inline]
    pub fn contains(&self, key: &K) -> bool {
        self.inner.contains_key(key)
    }

    /// Returns the number of elements in the set.
    #[inline]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns true if the set is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Iterate over keys in sorted order.
    ///
    /// ```
    /// use xmtp_mls_common::tls_set::TlsSet;
    ///
    /// let set = TlsSet::from_keys([3, 1, 2]);
    /// let keys: Vec<_> = set.iter().collect();
    /// assert_eq!(keys, vec![&1, &2, &3]);
    /// ```
    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = &K> {
        self.inner.keys()
    }
}

impl<K: Ord + Eq + Clone + Serialize + Size> TlsSet<K> {
    /// Apply a delta atomically. If any mutation fails, the set is unchanged.
    ///
    /// If the delta contains any `RemoveByHash` mutations, a hash index of all
    /// existing keys is built once (O(n)) and used for lookups (O(1) each),
    /// avoiding O(n*m) behavior. Returns [`TlsSetError::DuplicateHash`] if two
    /// existing keys produce the same SHA-256 hash.
    ///
    /// # Performance
    ///
    /// - **Time:** O(n + m) when the delta contains any `RemoveByHash`
    ///   (one O(n) index build, then O(1) per lookup), or O(m) otherwise,
    ///   where `n` is the current set size and `m` is the mutation count.
    /// - **Memory:** O(m) — all mutations are resolved into a temporary
    ///   `Vec<TlsMapMutation>` before being applied. This allocation is
    ///   required to preserve atomicity: resolution must complete before
    ///   any actual mutation happens, so a partial delta cannot leave the
    ///   set in an inconsistent state. When `RemoveByHash` is present, an
    ///   additional O(n) `HashMap` is allocated for the hash index, holding
    ///   borrowed key references (no key cloning during index build).
    ///
    /// ```
    /// use xmtp_mls_common::tls_set::{TlsSet, TlsSetDelta};
    ///
    /// let mut set = TlsSet::<u8>::new();
    /// set.insert(1).unwrap();
    ///
    /// let delta = TlsSetDelta::new().insert(2).remove(1);
    /// set.apply_delta(delta).unwrap();
    /// assert!(set.contains(&2));
    /// assert!(!set.contains(&1));
    /// ```
    pub fn apply_delta(&mut self, delta: TlsSetDelta<K>) -> Result<(), TlsSetError> {
        let has_hash_removal = delta
            .mutations
            .iter()
            .any(|m| matches!(m, TlsSetMutation::RemoveByHash(_)));

        // Build hash → &K index only when needed — O(n) once instead of
        // O(n) per removal. Stores borrowed references, no key cloning.
        let hash_index: Option<std::collections::HashMap<TlsKeyHash, &K>> = if has_hash_removal {
            let mut idx = std::collections::HashMap::with_capacity(self.inner.len());
            for key in self.inner.keys() {
                if idx.insert(TlsKeyHash::of(key)?, key).is_some() {
                    return Err(TlsSetError::DuplicateHash);
                }
            }
            Some(idx)
        } else {
            None
        };

        let resolved: Vec<TlsMapMutation<K, ()>> = delta
            .mutations
            .into_iter()
            .map(|m| -> Result<TlsMapMutation<K, ()>, TlsSetError> {
                match m {
                    TlsSetMutation::Insert(key) => Ok(TlsMapMutation::Insert { key, value: () }),
                    TlsSetMutation::Remove(key) => Ok(TlsMapMutation::Delete { key }),
                    TlsSetMutation::RemoveByHash(target_hash) => {
                        let idx = hash_index
                            .as_ref()
                            .expect("hash index must be built for RemoveByHash");
                        let key = idx.get(&target_hash).ok_or(TlsMapError::KeyNotFound)?;
                        Ok(TlsMapMutation::Delete {
                            key: (*key).clone(),
                        })
                    }
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        let map_delta = TlsMapDelta {
            mutations: resolved,
        };
        self.inner.apply_delta(map_delta)?;
        Ok(())
    }
}

// -- TLS codec --

impl<K: Size> Size for TlsSet<K> {
    #[inline]
    fn tls_serialized_len(&self) -> usize {
        self.inner.tls_serialized_len()
    }
}

impl<K: Serialize + Size + std::fmt::Debug> Serialize for TlsSet<K> {
    #[inline]
    fn tls_serialize<W: Write>(&self, writer: &mut W) -> Result<usize, tls_codec::Error> {
        self.inner.tls_serialize(writer)
    }
}

impl<K> Deserialize for TlsSet<K>
where
    K: Deserialize + Size + Ord + Eq,
{
    #[inline]
    fn tls_deserialize<R: Read>(bytes: &mut R) -> Result<Self, tls_codec::Error>
    where
        Self: Sized,
    {
        let inner = TlsMap::tls_deserialize(bytes)?;
        Ok(Self { inner })
    }
}

impl<K: Ord + Eq> FromIterator<K> for TlsSet<K> {
    #[inline]
    fn from_iter<T: IntoIterator<Item = K>>(iter: T) -> Self {
        Self::from_keys(iter)
    }
}

impl<K> IntoIterator for TlsSet<K> {
    type Item = K;
    type IntoIter = std::iter::Map<<TlsMap<K, ()> as IntoIterator>::IntoIter, fn((K, ())) -> K>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter().map(|(k, _)| k)
    }
}

// ============================================================================
// TlsSetDelta — atomic batch mutations for TlsSet
// ============================================================================

/// A mutation to apply to a [`TlsSet`].
///
/// Wire format: `u8(tag) || payload` where:
/// - tag 0 = Insert: `K(tls)`
/// - tag 1 = Remove: `K(tls)`
/// - tag 2 = RemoveByHash: `[u8; 32]` (SHA-256 of the key's TLS serialization)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TlsSetMutation<K> {
    /// Insert a key. Fails if the key already exists.
    Insert(K),
    /// Remove a key by value. Fails if the key doesn't exist.
    Remove(K),
    /// Remove a key by the SHA-256 hash of its TLS serialization.
    /// Avoids sending large keys over the wire for removal.
    /// Fails if no key with a matching hash exists.
    RemoveByHash(TlsKeyHash),
}

/// A batch of mutations to apply atomically to a [`TlsSet`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TlsSetDelta<K> {
    /// The ordered list of mutations to apply.
    pub mutations: Vec<TlsSetMutation<K>>,
}

impl<K> TlsSetDelta<K> {
    /// Create an empty delta.
    #[inline]
    pub fn new() -> Self {
        Self {
            mutations: Vec::new(),
        }
    }

    /// Append an insert mutation.
    #[inline]
    pub fn insert(mut self, key: K) -> Self {
        self.mutations.push(TlsSetMutation::Insert(key));
        self
    }

    /// Append a remove mutation.
    #[inline]
    pub fn remove(mut self, key: K) -> Self {
        self.mutations.push(TlsSetMutation::Remove(key));
        self
    }

    /// Append a remove-by-hash mutation.
    #[inline]
    pub fn remove_by_hash(mut self, hash: TlsKeyHash) -> Self {
        self.mutations.push(TlsSetMutation::RemoveByHash(hash));
        self
    }
}

impl<K> Default for TlsSetDelta<K> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

// -- TLS codec for delta types --

impl<K: Size> Size for TlsSetMutation<K> {
    #[inline]
    fn tls_serialized_len(&self) -> usize {
        1 + match self {
            Self::Insert(key) | Self::Remove(key) => key.tls_serialized_len(),
            Self::RemoveByHash(_) => 32,
        }
    }
}

impl<K: Serialize + Size> Serialize for TlsSetMutation<K> {
    #[inline]
    fn tls_serialize<W: Write>(&self, writer: &mut W) -> Result<usize, tls_codec::Error> {
        match self {
            Self::Insert(key) => {
                let mut written = 0_u8.tls_serialize(writer)?;
                written += key.tls_serialize(writer)?;
                Ok(written)
            }
            Self::Remove(key) => {
                let mut written = 1_u8.tls_serialize(writer)?;
                written += key.tls_serialize(writer)?;
                Ok(written)
            }
            Self::RemoveByHash(hash) => {
                let written = 2_u8.tls_serialize(writer)?;
                let bytes = hash.as_bytes();
                writer
                    .write_all(bytes)
                    .map_err(|e| tls_codec::Error::EncodingError(e.to_string()))?;
                Ok(written + bytes.len())
            }
        }
    }
}

impl<K: Deserialize + Size> Deserialize for TlsSetMutation<K> {
    #[inline]
    fn tls_deserialize<R: Read>(bytes: &mut R) -> Result<Self, tls_codec::Error>
    where
        Self: Sized,
    {
        let tag = u8::tls_deserialize(bytes)?;
        match tag {
            0 => Ok(Self::Insert(K::tls_deserialize(bytes)?)),
            1 => Ok(Self::Remove(K::tls_deserialize(bytes)?)),
            2 => {
                let mut buf = [0_u8; 32];
                bytes
                    .read_exact(&mut buf)
                    .map_err(|e| tls_codec::Error::DecodingError(e.to_string()))?;
                Ok(Self::RemoveByHash(TlsKeyHash::from_bytes(buf)))
            }
            _ => Err(tls_codec::Error::DecodingError(format!(
                "unknown TlsSetMutation tag: {tag}"
            ))),
        }
    }
}

impl<K: Size> Size for TlsSetDelta<K> {
    #[inline]
    fn tls_serialized_len(&self) -> usize {
        self.mutations.tls_serialized_len()
    }
}

impl<K: Serialize + Size + std::fmt::Debug> Serialize for TlsSetDelta<K> {
    #[inline]
    fn tls_serialize<W: Write>(&self, writer: &mut W) -> Result<usize, tls_codec::Error> {
        self.mutations.tls_serialize(writer)
    }
}

impl<K: Deserialize + Size> Deserialize for TlsSetDelta<K> {
    #[inline]
    fn tls_deserialize<R: Read>(bytes: &mut R) -> Result<Self, tls_codec::Error>
    where
        Self: Sized,
    {
        let mutations = Vec::<TlsSetMutation<K>>::tls_deserialize(bytes)?;
        Ok(Self { mutations })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tls_codec::{Deserialize, Serialize};

    #[xmtp_common::test]
    fn test_insert_and_contains() {
        let mut set = TlsSet::<u16>::new();
        set.insert(42).unwrap();
        assert!(set.contains(&42));
        assert!(!set.contains(&99));
    }

    #[xmtp_common::test]
    fn test_insert_duplicate_fails() {
        let mut set = TlsSet::<u8>::new();
        set.insert(1).unwrap();
        assert!(set.insert(1).is_err());
    }

    #[xmtp_common::test]
    fn test_remove() {
        let mut set = TlsSet::<u8>::new();
        set.insert(1).unwrap();
        set.remove(&1).unwrap();
        assert!(!set.contains(&1));
        assert!(set.is_empty());
    }

    #[xmtp_common::test]
    fn test_remove_missing_fails() {
        let mut set = TlsSet::<u8>::new();
        assert!(set.remove(&1).is_err());
    }

    #[xmtp_common::test]
    fn test_from_keys_deduplicates() {
        let set = TlsSet::from_keys([1_u8, 2, 3, 1, 2]);
        assert_eq!(set.len(), 3);
    }

    #[xmtp_common::test]
    fn test_iter_sorted() {
        let set = TlsSet::from_keys([3_u8, 1, 2]);
        let keys: Vec<_> = set.iter().copied().collect();
        assert_eq!(keys, vec![1, 2, 3]);
    }

    #[xmtp_common::test]
    fn test_tls_round_trip() {
        let set = TlsSet::from_keys([10_u16, 20, 30]);
        let bytes = set.tls_serialize_detached().unwrap();
        let restored = TlsSet::<u16>::tls_deserialize_exact(&bytes).unwrap();
        assert_eq!(set, restored);
    }

    #[xmtp_common::test]
    fn test_empty_round_trip() {
        let set = TlsSet::<u8>::new();
        let bytes = set.tls_serialize_detached().unwrap();
        let restored = TlsSet::<u8>::tls_deserialize_exact(&bytes).unwrap();
        assert_eq!(set, restored);
        assert!(restored.is_empty());
    }

    #[xmtp_common::test]
    fn test_into_iter() {
        let set = TlsSet::from_keys([3_u8, 1, 2]);
        let keys: Vec<_> = set.into_iter().collect();
        assert_eq!(keys, vec![1, 2, 3]);
    }

    #[xmtp_common::test]
    fn test_collect() {
        let set: TlsSet<u8> = [3, 1, 2].into_iter().collect();
        assert_eq!(set.len(), 3);
        assert!(set.contains(&1));
    }

    #[xmtp_common::test]
    fn test_apply_delta() {
        let mut set = TlsSet::from_keys([1_u8, 2, 3]);
        let delta = TlsSetDelta::new().insert(4).remove(2);
        set.apply_delta(delta).unwrap();
        assert!(set.contains(&1));
        assert!(!set.contains(&2));
        assert!(set.contains(&3));
        assert!(set.contains(&4));
    }

    #[xmtp_common::test]
    fn test_apply_delta_rollback_on_failure() {
        let mut set = TlsSet::from_keys([1_u8, 2]);
        // Try to add 3 then add 1 (duplicate) — should rollback
        let delta = TlsSetDelta::new().insert(3).insert(1);
        assert!(set.apply_delta(delta).is_err());
        // Set should be unchanged
        assert_eq!(set.len(), 2);
        assert!(!set.contains(&3));
    }

    #[xmtp_common::test]
    fn test_remove_by_hash() {
        let mut set = TlsSet::from_keys([10_u16, 20, 30]);
        let hash = TlsKeyHash::of(&20_u16).unwrap();
        let delta = TlsSetDelta::new().remove_by_hash(hash);
        set.apply_delta(delta).unwrap();
        assert!(set.contains(&10));
        assert!(!set.contains(&20));
        assert!(set.contains(&30));
    }

    #[xmtp_common::test]
    fn test_remove_by_hash_not_found() {
        let mut set = TlsSet::from_keys([10_u16, 20]);
        let hash = TlsKeyHash::of(&99_u16).unwrap(); // not in set
        let delta = TlsSetDelta::new().remove_by_hash(hash);
        assert!(set.apply_delta(delta).is_err());
    }

    #[xmtp_common::test]
    fn test_remove_by_hash_matches_remove_by_value() {
        let mut set_a = TlsSet::from_keys([1_u16, 2, 3]);
        let mut set_b = set_a.clone();

        // Remove by value
        let delta_a = TlsSetDelta::new().remove(2);
        set_a.apply_delta(delta_a).unwrap();

        // Remove by hash
        let hash = TlsKeyHash::of(&2_u16).unwrap();
        let delta_b = TlsSetDelta::new().remove_by_hash(hash);
        set_b.apply_delta(delta_b).unwrap();

        assert_eq!(set_a, set_b);
    }

    #[xmtp_common::test]
    fn test_delta_tls_round_trip() {
        let delta = TlsSetDelta::<u16>::new().insert(10).remove(20).insert(30);
        let bytes = delta.tls_serialize_detached().unwrap();
        let restored = TlsSetDelta::<u16>::tls_deserialize_exact(&bytes).unwrap();
        assert_eq!(delta, restored);
    }

    #[xmtp_common::test]
    fn test_remove_by_hash_mutation_tls_round_trip() {
        let hash = TlsKeyHash::of(&42_u16).unwrap();
        let mutation = TlsSetMutation::<u16>::RemoveByHash(hash);
        let bytes = mutation.tls_serialize_detached().unwrap();
        let restored = TlsSetMutation::<u16>::tls_deserialize_exact(&bytes).unwrap();
        assert_eq!(mutation, restored);
    }

    #[xmtp_common::test]
    fn test_mutation_tls_round_trip() {
        let add = TlsSetMutation::Insert(42_u16);
        let bytes = add.tls_serialize_detached().unwrap();
        let restored = TlsSetMutation::<u16>::tls_deserialize_exact(&bytes).unwrap();
        assert_eq!(add, restored);

        let remove = TlsSetMutation::Remove(99_u16);
        let bytes = remove.tls_serialize_detached().unwrap();
        let restored = TlsSetMutation::<u16>::tls_deserialize_exact(&bytes).unwrap();
        assert_eq!(remove, restored);
    }

    #[xmtp_common::test]
    fn test_deserialize_unknown_tag() {
        // Tag 3 is not a valid TlsSetMutation variant (only 0/1/2 are defined).
        let bytes = [3_u8, 0, 42];
        let result = TlsSetMutation::<u16>::tls_deserialize_exact(bytes);
        assert!(matches!(result, Err(tls_codec::Error::DecodingError(_))));
    }

    /// A key type with a deliberately buggy `Serialize` impl that produces
    /// identical bytes for distinct logical values. Used to exercise the
    /// duplicate-hash detection path in `apply_delta`, since SHA-256 collisions
    /// cannot be found by chance with well-formed inputs.
    #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
    struct CollidingKey(u16);

    impl Size for CollidingKey {
        fn tls_serialized_len(&self) -> usize {
            1
        }
    }

    impl Serialize for CollidingKey {
        fn tls_serialize<W: std::io::Write>(
            &self,
            writer: &mut W,
        ) -> Result<usize, tls_codec::Error> {
            // Buggy: always serialize as the same byte regardless of value.
            0_u8.tls_serialize(writer)
        }
    }

    impl Deserialize for CollidingKey {
        fn tls_deserialize<R: std::io::Read>(bytes: &mut R) -> Result<Self, tls_codec::Error>
        where
            Self: Sized,
        {
            let _ = u8::tls_deserialize(bytes)?;
            Ok(CollidingKey(0))
        }
    }

    #[xmtp_common::test]
    fn test_apply_delta_duplicate_hash() {
        let mut set = TlsSet::<CollidingKey>::new();
        // Distinct logical keys but identical TLS serialization → identical hashes.
        set.insert(CollidingKey(1)).unwrap();
        set.insert(CollidingKey(2)).unwrap();

        let any_hash = TlsKeyHash::of(&CollidingKey(0)).unwrap();
        let delta = TlsSetDelta::new().remove_by_hash(any_hash);
        let result = set.apply_delta(delta);
        assert!(matches!(result, Err(TlsSetError::DuplicateHash)));
    }

    #[xmtp_common::test]
    fn test_apply_delta_remove_by_hash_not_found_in_index() {
        // Build the hash index successfully (no collisions), then look up a
        // hash that doesn't match any key. This exercises the
        // `idx.get(...).ok_or(KeyNotFound)?` path inside the resolve loop.
        let mut set = TlsSet::<u16>::new();
        set.insert(10).unwrap();
        set.insert(20).unwrap();

        let missing_hash = TlsKeyHash::of(&999_u16).unwrap();
        let delta = TlsSetDelta::new().remove_by_hash(missing_hash);
        let result = set.apply_delta(delta);
        assert!(matches!(
            result,
            Err(TlsSetError::Map(TlsMapError::KeyNotFound))
        ));
        // Set is unchanged.
        assert!(set.contains(&10));
        assert!(set.contains(&20));
    }
}
