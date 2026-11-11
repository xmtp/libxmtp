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
/// `$n` bounds generated pairs. Counts above the key space test duplicate keys.
macro_rules! tls_map_tests {
    ($mod_name:ident, $K:ty, $V:ty, $n:expr, $mutate_v:expr) => {
        mod $mod_name {
            use super::*;

            proptest! {
                #[test]
                fn round_trip(
                    pairs in proptest::collection::vec((any::<$K>(), any::<$V>()), 0..$n),
                    single_key in any::<$K>(),
                    single_value in any::<$V>(),
                ) {
                    let map = build_map(pairs.clone().into_iter());
                    let bytes = map.tls_serialize_detached().expect("round trip serialization should succeed");
                    prop_assert_eq!(bytes.len(), map.tls_serialized_len(), "serialized size");
                    let deserialized =
                        TlsMap::<$K, $V>::tls_deserialize_exact(&bytes).expect("round trip deserialization should succeed");
                    prop_assert_eq!(map, deserialized);

                    let from_pairs = TlsMap::from_pairs(pairs.clone());
                    let collected = pairs.clone().into_iter().collect::<TlsMap<$K, $V>>();
                    let mut set_map = TlsMap::new();
                    for (key, value) in pairs {
                        set_map.set(key, value);
                    }
                    let canonical = from_pairs.tls_serialize_detached().unwrap();
                    prop_assert_eq!(&canonical, &collected.tls_serialize_detached().unwrap(), "collect encoding");
                    prop_assert_eq!(&canonical, &set_map.tls_serialize_detached().unwrap(), "set encoding");

                    let mut single = TlsMap::<$K, $V>::new();
                    single.insert(single_key, single_value).unwrap();
                    let single_bytes = single.tls_serialize_detached().unwrap();
                    prop_assert_eq!(single_bytes.len(), single.tls_serialized_len(), "single-entry size");
                    prop_assert_eq!(
                        single,
                        TlsMap::<$K, $V>::tls_deserialize_exact(&single_bytes).unwrap(),
                        "single-entry round trip"
                    );
                    prop_assert_eq!(TlsMap::<$K, $V>::new().tls_serialize_detached().unwrap(), vec![0], "empty encoding");
                }

                #[test]
                // verifies: META-012
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
                // verifies: META-012
                fn keys_always_sorted(
                    pairs in proptest::collection::vec((any::<$K>(), any::<$V>()), 0..$n)
                ) {
                    let expected: std::collections::HashMap<$K, $V> = pairs.clone().into_iter().collect();
                    let map = build_map(pairs.into_iter());
                    let keys: Vec<&$K> = map.keys().collect();
                    for w in keys.array_windows::<2>() {
                        prop_assert!(w[0] < w[1]);
                    }
                    let borrowed: Vec<_> = map.iter().collect();
                    prop_assert_eq!(borrowed.len(), map.len(), "borrowed pairs");
                    for (key, value) in &borrowed {
                        prop_assert_eq!(expected.get(*key), Some(*value), "borrowed pair value");
                    }
                    prop_assert_eq!(map.values().collect::<Vec<_>>(), borrowed.iter().map(|(_, v)| *v).collect::<Vec<_>>(), "borrowed values");
                    let owned: Vec<_> = map.clone().into_iter().collect();
                    for w in owned.array_windows::<2>() {
                        prop_assert!(w[0].0 < w[1].0, "owned order");
                    }
                    prop_assert_eq!(owned.len(), borrowed.len(), "owned pairs");
                    for ((key, value), (borrowed_key, borrowed_value)) in owned.iter().zip(&borrowed) {
                        prop_assert_eq!(key, *borrowed_key, "owned key");
                        prop_assert_eq!(value, *borrowed_value, "owned value");
                    }
                    let mut one = TlsMap::<$K, $V>::new();
                    let key = <$K>::default();
                    let value = <$V>::default();
                    one.set(key.clone(), value.clone());
                    prop_assert_eq!(one.iter().collect::<Vec<_>>(), vec![(&key, &value)], "one borrowed pair");
                    prop_assert_eq!(one.values().collect::<Vec<_>>(), vec![&value], "one borrowed value");
                }

                #[test]
                fn get_returns_inserted(
                    pairs in proptest::collection::vec((any::<$K>(), any::<$V>()), 1..$n)
                ) {
                    let mut map = build_map(pairs.clone().into_iter());
                    let expected: std::collections::HashMap<$K, $V> = pairs
                        .into_iter()
                        .collect();
                    prop_assert_eq!(map.len(), expected.len());
                    for (k, v) in &expected {
                        prop_assert_eq!(map.get(k), Some(v));
                        let changed = ($mutate_v)(v.clone());
                        prop_assert!(matches!(map.insert(k.clone(), changed), Err(TlsMapError::KeyExists)), "duplicate insert");
                    }
                    let mut removed = map.clone();
                    for (k, v) in &expected {
                        prop_assert_eq!(&removed.remove(k).unwrap(), v, "remove returns value");
                    }
                    prop_assert!(removed.is_empty(), "all keys removed");
                    if let Some((key, value)) = expected.iter().next() {
                        let changed = ($mutate_v)(value.clone());
                        *map.get_mut(key).unwrap() = changed.clone();
                        prop_assert_eq!(map.get(key), Some(&changed), "mutable get");
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

                /// Reject bytes where two entries are serialized in descending key order.
                #[test]
                // verifies: META-012
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
                // verifies: META-012
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
                    reverse_entries in proptest::collection::hash_map(
                        any::<$K>(),
                        proptest::collection::hash_map(any::<u64>(), any::<$V>(), 0..20),
                        1..std::cmp::min($n, 10)
                    ),
                ) {
                    let outer: TlsMap<u64, TlsMap<$K, $V>> = entries
                        .into_iter()
                        .map(|(k, inner)| (k, inner.into_iter().collect()))
                        .collect();
                    let bytes = outer.tls_serialize_detached().unwrap();
                    let deserialized = TlsMap::<u64, TlsMap<$K, $V>>::tls_deserialize_exact(&bytes).unwrap();
                    prop_assert_eq!(outer, deserialized, "u64 outer keys");

                    let reverse_outer: TlsMap<$K, TlsMap<u64, $V>> = reverse_entries
                        .into_iter()
                        .map(|(k, inner)| (k, inner.into_iter().collect()))
                        .collect();
                    let reverse_bytes = reverse_outer.tls_serialize_detached().unwrap();
                    let reverse_result = TlsMap::<$K, TlsMap<u64, $V>>::tls_deserialize_exact(&reverse_bytes).unwrap();
                    prop_assert_eq!(reverse_outer, reverse_result, "parameterized outer keys");
                }

                /// Delta (list of mutations) round-trips through serialization.
                #[test]
                fn delta_round_trip(
                    mutations in proptest::collection::vec(
                        (any::<$K>(), any::<$V>(), 0u8..3), 0..20
                    ),
                    key in any::<$K>(),
                    value in any::<$V>(),
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
                    for mutation in [
                        TlsMapMutation::Insert { key: key.clone(), value: value.clone() },
                        TlsMapMutation::Update { key: key.clone(), value: value.clone() },
                        TlsMapMutation::Delete { key: key.clone() },
                    ] {
                        let bytes = mutation.tls_serialize_detached().unwrap();
                        let decoded = TlsMapMutation::<$K, $V>::tls_deserialize_exact(&bytes).unwrap();
                        prop_assert_eq!(mutation, decoded, "mutation kind round trip");
                    }
                    let key_len = key.tls_serialized_len();
                    let mut key_only = Vec::new();
                    tls_codec::vlen::write_length(&mut key_only, 1 + key_len).unwrap();
                    3u8.tls_serialize(&mut key_only).unwrap();
                    key.tls_serialize(&mut key_only).unwrap();
                    prop_assert!(TlsMapDelta::<$K, $V>::tls_deserialize_exact(&key_only).is_err(), "invalid mutation tag with key only");

                    let mut key_and_value = Vec::new();
                    tls_codec::vlen::write_length(&mut key_and_value, 1 + key_len + value.tls_serialized_len()).unwrap();
                    3u8.tls_serialize(&mut key_and_value).unwrap();
                    key.tls_serialize(&mut key_and_value).unwrap();
                    value.tls_serialize(&mut key_and_value).unwrap();
                    prop_assert!(TlsMapDelta::<$K, $V>::tls_deserialize_exact(&key_and_value).is_err(), "invalid mutation tag with key and value");
                }
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

/// Apply one mutation to the model map, or name the error it fails with.
fn apply_to_model(
    model: &mut std::collections::BTreeMap<u8, u8>,
    mutation: &TlsMapMutation<u8, u8>,
) -> Result<(), &'static str> {
    match *mutation {
        TlsMapMutation::Insert { key, value } => match model.entry(key) {
            std::collections::btree_map::Entry::Occupied(_) => Err("KeyExists"),
            std::collections::btree_map::Entry::Vacant(slot) => {
                slot.insert(value);
                Ok(())
            }
        },
        TlsMapMutation::Update { key, value } => match model.get_mut(&key) {
            Some(slot) => {
                *slot = value;
                Ok(())
            }
            None => Err("KeyNotFound"),
        },
        TlsMapMutation::Delete { key } => model.remove(&key).map(drop).ok_or("KeyNotFound"),
    }
}

fn error_name(error: TlsMapError) -> &'static str {
    match error {
        TlsMapError::KeyExists => "KeyExists",
        TlsMapError::KeyNotFound => "KeyNotFound",
        TlsMapError::Codec(_) => "Codec",
    }
}

proptest! {
    /// A delta has the result of applying its mutations one at a time to a
    /// copy of the map: the same final map, the error of the first mutation
    /// that fails, and an unchanged map on error. Each step picks a mutation
    /// that succeeds or fails against the copy, so failures land anywhere in
    /// the delta and keys repeat within it.
    #[test]
    // verifies: META-013
    fn delta_matches_sequential_model(
        base in proptest::collection::btree_map(0u8..32, any::<u8>(), 0..32),
        steps in proptest::collection::vec(
            (0u8..32, any::<u8>(), proptest::bool::weighted(0.97), any::<bool>()),
            0..=64,
        ),
    ) {
        let mut model = base.clone();
        let mut expected = Ok(());
        let mut delta = TlsMapDelta::new();
        for (key, value, succeeds, update) in steps {
            // An update or delete succeeds on a present key, an insert on an
            // absent one.
            let mutation = if model.contains_key(&key) == succeeds {
                if update {
                    TlsMapMutation::Update { key, value }
                } else {
                    TlsMapMutation::Delete { key }
                }
            } else {
                TlsMapMutation::Insert { key, value }
            };
            if expected.is_ok() {
                expected = apply_to_model(&mut model, &mutation);
            }
            delta.mutations.push(mutation);
        }
        if expected.is_err() {
            model = base.clone();
        }

        let mut map = TlsMap::from_pairs(base);
        prop_assert_eq!(map.apply_delta(delta).map_err(error_name), expected);
        prop_assert_eq!(map.into_iter().collect::<Vec<_>>(), model.into_iter().collect::<Vec<_>>());
    }
}
