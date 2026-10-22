//! Legacy metadata encodings. A std-only reader decodes the stored group
//! context; fixtures from the pinned legacy libraries check it.
use crate::legacy_context::LegacyContext;
use prost::Message;
use tls_codec::Deserialize;
use xmtp_mls_common::{
    group_mutable_metadata::{
        GroupMutableMetadata, MetadataField, merge_dict_entries_into_mutable_metadata_lossy,
    },
    inbox_id::InboxId,
};
use xmtp_proto::xmtp::{
    device_sync::group_backup::{ImmutableMetadataSave, MutableMetadataSave},
    mls::message_contents::{GroupMetadataV1, GroupMutableMetadataV1},
};
use zeroize::Zeroize;

// Wire IDs are pinned to 6a8e9697 (legacy metadata) and cc878025 (AppData).
// Keep these IDs when current protocol definitions change. See fixtures/README.md.
const MUTABLE_METADATA_EXTENSION: u16 = 0xff00;
const COMPONENT_REGISTRY: u16 = 0x8000;
const CREATOR: u16 = 0xbffe;
// Optional metadata has a fixed byte budget. Larger contexts become unknown.
pub(crate) const MAX_CONTEXT_BYTES: usize = 1024 * 1024;

/// The source store applies its label and version wrapper twice.
pub(crate) fn context_key(id: &[u8]) -> Vec<u8> {
    let mut key = b"GroupContextGroupContext".to_vec();
    key.extend_from_slice(&(id.len() as u64).to_le_bytes());
    key.extend_from_slice(id);
    key.extend_from_slice(&[0, 1, 0, 1]);
    key
}

/// Decode each optional component independently. Required SQL identity is read
/// elsewhere and never depends on this result.
// implements: MIG-005
pub(crate) fn decode(bytes: &[u8]) -> (Option<ImmutableMetadataSave>, Option<MutableMetadataSave>) {
    if bytes.len() > MAX_CONTEXT_BYTES {
        return (None, None);
    }
    let Some(context) = LegacyContext::read(bytes) else {
        return (None, None);
    };
    let mut immutable = context.immutable_metadata.and_then(decode_legacy_immutable);
    let mut mutable = context
        .unknown
        .iter()
        .filter(|(id, _)| *id == MUTABLE_METADATA_EXTENSION)
        .find_map(|(_, bytes)| decode_legacy_mutable(bytes))
        .map(without_commit_log_signer);
    if let Some(dict) = &context.app_data
        && dict.contains_key(&COMPONENT_REGISTRY)
    {
        immutable = dict
            .get(&CREATOR)
            .and_then(|bytes| match InboxId::tls_deserialize_exact(bytes) {
                Ok(id) => Some(ImmutableMetadataSave {
                    creator_inbox_id: hex::encode(id.as_bytes()),
                }),
                Err(_) => None,
            })
            .or(immutable);
        let mut result = GroupMutableMetadata::new(Default::default(), vec![], vec![]);
        let errors = merge_dict_entries_into_mutable_metadata_lossy(&mut result, |id| {
            dict.get(&id).copied()
        });
        result.remove_attribute(MetadataField::CommitLogSigner.as_str());
        if !errors.is_empty() {
            tracing::debug!(
                count = errors.len(),
                "Cannot decode optional legacy metadata fields"
            );
        }
        mutable = Some(MutableMetadataSave {
            attributes: result.take_attributes(),
            admin_list: std::mem::take(&mut result.admin_list),
            super_admin_list: std::mem::take(&mut result.super_admin_list),
        });
    }
    (immutable, mutable)
}

/// Legacy groups kept the commit-log signer's private key in mutable
/// metadata. An archive never carries it, so drop and wipe it here.
fn without_commit_log_signer(mut save: MutableMetadataSave) -> MutableMetadataSave {
    if let Some(mut value) = save
        .attributes
        .remove(MetadataField::CommitLogSigner.as_str())
    {
        value.zeroize();
    }
    save
}

fn decode_legacy_immutable(bytes: &[u8]) -> Option<ImmutableMetadataSave> {
    if let Ok(value) = GroupMetadataV1::decode(bytes) {
        return (!value.creator_inbox_id.is_empty()).then_some(ImmutableMetadataSave {
            creator_inbox_id: value.creator_inbox_id,
        });
    }
    let mut creator = None;
    protobuf_fields(bytes, |tag, raw, _| {
        if tag == 3 {
            creator = GroupMetadataV1::decode(raw).ok().and_then(|value| {
                (!value.creator_inbox_id.is_empty()).then_some(ImmutableMetadataSave {
                    creator_inbox_id: value.creator_inbox_id,
                })
            });
        }
    });
    creator
}

/// Keep the normal decoder for valid metadata. Recover fields only when Prost
/// can find their complete boundaries. An invalid boundary ends recovery.
fn decode_legacy_mutable(bytes: &[u8]) -> Option<MutableMetadataSave> {
    use xmtp_proto::xmtp::mls::message_contents::Inboxes;

    // The proto wipes the attribute values it still holds when it drops, so
    // fields move out with `take`.
    if let Ok(mut value) = GroupMutableMetadataV1::decode(bytes) {
        return Some(MutableMetadataSave {
            attributes: std::mem::take(&mut value.attributes),
            admin_list: value
                .admin_list
                .take()
                .map(|v| v.inbox_ids)
                .unwrap_or_default(),
            super_admin_list: value
                .super_admin_list
                .take()
                .map(|v| v.inbox_ids)
                .unwrap_or_default(),
        });
    }
    let mut result = MutableMetadataSave::default();
    let mut recovered = false;
    protobuf_fields(bytes, |tag, raw, payload| {
        match tag {
            1 => match GroupMutableMetadataV1::decode(raw) {
                Ok(mut value) => {
                    result
                        .attributes
                        .extend(std::mem::take(&mut value.attributes));
                    recovered = true;
                }
                Err(_) => {
                    // A later malformed value must not keep an older value for
                    // the same key. The key decoder skips the value bytes.
                    #[derive(prost::Message)]
                    struct AttributeKey {
                        #[prost(string, tag = "1")]
                        key: String,
                    }
                    if let Some(payload) = payload
                        && let Ok(key) = AttributeKey::decode(payload)
                    {
                        result.attributes.remove(&key.key);
                    }
                }
            },
            2 | 3 => {
                let Some(payload) = payload else {
                    return;
                };
                let list = if tag == 2 {
                    &mut result.admin_list
                } else {
                    &mut result.super_admin_list
                };
                if let Ok(value) = Inboxes::decode(payload) {
                    list.extend(value.inbox_ids);
                    recovered = true;
                } else {
                    protobuf_fields(payload, |tag, raw, _| {
                        if tag == 1
                            && let Ok(value) = Inboxes::decode(raw)
                        {
                            list.extend(value.inbox_ids);
                            recovered = true;
                        }
                    });
                }
            }
            _ => {}
        }
    });
    recovered.then_some(result)
}

/// Visit complete fields without allocating their payloads. Do not scan for a
/// new field after a malformed length or key: its position is unknown.
fn protobuf_fields(mut bytes: &[u8], mut visit: impl FnMut(u32, &[u8], Option<&[u8]>)) {
    use prost::encoding::{DecodeContext, WireType, decode_key, decode_varint, skip_field};

    while !bytes.is_empty() {
        let start = bytes;
        let Ok((tag, wire_type)) = decode_key(&mut bytes) else {
            break;
        };
        let mut payload = bytes;
        if skip_field(wire_type, tag, &mut bytes, DecodeContext::default()).is_err() {
            break;
        }
        let raw = &start[..start.len() - bytes.len()];
        let payload = if wire_type == WireType::LengthDelimited {
            decode_varint(&mut payload)
                .ok()
                .and_then(|length| usize::try_from(length).ok())
                .and_then(|length| payload.get(..length))
        } else {
            None
        };
        visit(tag, raw, payload);
    }
}

#[cfg(test)]
mod tests {
    use super::{ImmutableMetadataSave, MutableMetadataSave};
    use prost::Message;
    use serde_json::{Value, json};
    use xmtp_proto::xmtp::mls::message_contents::{GroupMutableMetadataV1, Inboxes};

    /// Group contexts written with the legacy OpenMLS fork types and bincode,
    /// and the metadata each one must decode to. A decoder built on those
    /// types made them while the fork was pinned. The corpus is now frozen.
    const CORPUS: &str = include_str!("../fixtures/legacy-context-corpus.json");

    struct Case {
        name: String,
        context: Vec<u8>,
        expected: Value,
    }

    fn corpus() -> Vec<Case> {
        let corpus: Value = serde_json::from_str(CORPUS).unwrap();
        corpus["cases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|case| Case {
                name: case["name"].as_str().unwrap().to_owned(),
                context: hex::decode(case["context"].as_str().unwrap()).unwrap(),
                expected: case["expected"].clone(),
            })
            .collect()
    }

    fn context(name: &str) -> Vec<u8> {
        corpus()
            .into_iter()
            .find(|case| case.name == name)
            .unwrap_or_else(|| panic!("no corpus case {name}"))
            .context
    }

    /// Decoded metadata in the corpus's JSON form.
    fn summary(
        (immutable, mutable): &(Option<ImmutableMetadataSave>, Option<MutableMetadataSave>),
    ) -> Value {
        json!({
            "creator": immutable.as_ref().map(|value| &value.creator_inbox_id),
            "mutable": mutable.as_ref().map(|value| json!({
                // Sorted, so a rebuild writes the same bytes.
                "attributes": value.attributes.iter().collect::<std::collections::BTreeMap<_, _>>(),
                "admin_list": value.admin_list,
                "super_admin_list": value.super_admin_list,
            })),
        })
    }

    /// The offset of the tree hash length in a context with protocol version
    /// Mls10: two u32 tags, the group id, and the u64 epoch come first.
    fn tree_hash_offset(context: &[u8]) -> usize {
        let group_id = u64::from_le_bytes(context[8..16].try_into().unwrap());
        16 + usize::try_from(group_id).unwrap() + 8
    }

    fn with_tree_hash(context: &[u8], length: usize) -> Vec<u8> {
        let start = tree_hash_offset(context);
        let old = u64::from_le_bytes(context[start..start + 8].try_into().unwrap());
        let rest = start + 8 + usize::try_from(old).unwrap();
        let mut bytes = context[..start].to_vec();
        bytes.extend((length as u64).to_le_bytes());
        bytes.resize(bytes.len() + length, 0);
        bytes.extend_from_slice(&context[rest..]);
        bytes
    }

    fn field(tag: u32, bytes: &[u8]) -> Vec<u8> {
        let mut encoded = vec![];
        prost::encoding::encode_key(
            tag,
            prost::encoding::WireType::LengthDelimited,
            &mut encoded,
        );
        prost::encoding::encode_varint(bytes.len() as u64, &mut encoded);
        encoded.extend_from_slice(bytes);
        encoded
    }

    // verifies: MIG-005
    #[xmtp_common::test(unwrap_try = true)]
    fn corpus_contexts_decode_to_recorded_metadata() {
        let cases = corpus();
        assert!(!cases.is_empty());
        let failures: Vec<_> = cases
            .iter()
            .filter(|case| summary(&super::decode(&case.context)) != case.expected)
            .map(|case| case.name.as_str())
            .collect();
        assert!(failures.is_empty(), "wrong metadata for {failures:?}");
    }

    /// A cut context is unreadable: every proper prefix of every corpus case
    /// has no metadata.
    // verifies: MIG-005
    #[xmtp_common::test(unwrap_try = true)]
    fn cut_contexts_have_no_metadata() {
        let mut failures = vec![];
        for case in corpus() {
            for length in 0..case.context.len() {
                if super::decode(&case.context[..length]) != (None, None) {
                    failures.push(format!("{} cut to {length}", case.name));
                }
            }
        }
        assert!(
            failures.is_empty(),
            "a cut context has metadata: {failures:?}"
        );
    }

    /// Each byte of a context with every readable extension type, changed
    /// three ways, decodes without a panic and to the fork decoder's result,
    /// frozen in `fixtures/legacy-context-mutations.json`: the listed
    /// changes yield their metadata, and every other change yields none.
    // verifies: MIG-005
    #[xmtp_common::test(unwrap_try = true)]
    fn changed_bytes_do_not_panic() {
        let frozen: Value =
            serde_json::from_str(include_str!("../fixtures/legacy-context-mutations.json"))?;
        let results = frozen["results"].as_array().unwrap();
        let mut expected = std::collections::BTreeMap::new();
        for mutation in frozen["mutations"].as_array().unwrap() {
            let index = mutation[0].as_u64().unwrap() as usize;
            let value = mutation[1].as_u64().unwrap() as u8;
            let result = &results[mutation[2].as_u64().unwrap() as usize];
            expected.insert((index, value), result);
        }
        assert!(expected.len() > 1_000, "only {} changes", expected.len());

        let none = summary(&(None, None));
        let everything = context("everything");
        let mut failures = vec![];
        for (index, &byte) in everything.iter().enumerate() {
            for value in [0x00, 0xff, byte ^ 0x01] {
                let mut bytes = everything.clone();
                bytes[index] = value;
                let decoded = summary(&super::decode(&bytes));
                if decoded != **expected.get(&(index, value)).unwrap_or(&&none) {
                    failures.push(format!("everything[{index}] = {value:#04x}"));
                }
            }
        }
        assert!(failures.is_empty(), "changed results: {failures:?}");
    }

    /// Every group context in the SQLite fixtures, read from temporary copies.
    /// `early.db3` has no OpenMLS rows, and the encrypted fixtures need a key.
    #[cfg(not(target_arch = "wasm32"))]
    fn fixture_database_contexts(name: &str) -> Vec<Vec<u8>> {
        use diesel::{
            Connection, QueryableByName, RunQueryDsl, SqliteConnection, sql_types::Binary,
        };

        #[derive(QueryableByName)]
        struct Row {
            #[diesel(sql_type = Binary)]
            key_bytes: Vec<u8>,
            #[diesel(sql_type = Binary)]
            value_bytes: Vec<u8>,
        }

        let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures");
        let directory = tempfile::tempdir().unwrap();
        let copy = directory.path().join(name);
        std::fs::copy(fixtures.join(name), &copy).unwrap();
        let mut conn = SqliteConnection::establish(copy.to_str().unwrap()).unwrap();
        let rows: Vec<Row> =
            diesel::sql_query("SELECT key_bytes, value_bytes FROM openmls_key_value")
                .load(&mut conn)
                .unwrap();
        rows.into_iter()
            .filter(|row| row.key_bytes.starts_with(b"GroupContextGroupContext"))
            .map(|row| row.value_bytes)
            .collect()
    }

    /// The real contexts in the fixture databases have metadata, and every cut
    /// of them has none. The two seed databases hold the producers' own
    /// output, with the values the producers recorded.
    // verifies: MIG-005
    #[cfg(not(target_arch = "wasm32"))]
    #[xmtp_common::test(unwrap_try = true)]
    fn fixture_database_contexts_decode() {
        let expected: Value =
            serde_json::from_str(include_str!("../fixtures/metadata-expected.json"))?;
        let mut failures = vec![];
        let mut seeds = vec![];
        for name in [
            "metadata-seed.db3",
            "appdata-seed.db3",
            "stable.db3",
            "consent-states.db3",
            "mobile-4.10.db3",
        ] {
            let contexts = fixture_database_contexts(name);
            assert!(!contexts.is_empty(), "{name} has no group context");
            for (index, bytes) in contexts.iter().enumerate() {
                let decoded = super::decode(bytes);
                assert_ne!(
                    decoded,
                    (None, None),
                    "{name} context {index} has no metadata"
                );
                if name.ends_with("-seed.db3") {
                    seeds.push((name, decoded));
                }
                for length in 0..bytes.len() {
                    if super::decode(&bytes[..length]) != (None, None) {
                        failures.push(format!("{name} context {index} cut to {length}"));
                    }
                }
            }
        }
        assert!(
            failures.is_empty(),
            "a cut context has metadata: {failures:?}"
        );

        let seed = |name: &str, group_name: &str| {
            seeds.iter().find(|(seed, (_, mutable))| {
                *seed == name
                    && mutable
                        .as_ref()
                        .map(|value| value.attributes["group_name"].as_str())
                        == Some(group_name)
            })
        };
        let attributes = &expected["attributes"];
        let (_, (immutable, mutable)) = seed(
            "metadata-seed.db3",
            attributes["group_name"].as_str().unwrap(),
        )
        .expect("the metadata seed context");
        assert_eq!(
            immutable.as_ref().unwrap().creator_inbox_id,
            expected["creator_inbox_id"].as_str().unwrap()
        );
        assert_eq!(
            mutable.as_ref().unwrap().attributes["description"],
            attributes["description"].as_str().unwrap()
        );
        let (_, (immutable, mutable)) =
            seed("appdata-seed.db3", "AppData Group").expect("the AppData seed context");
        assert_eq!(
            immutable.as_ref().unwrap().creator_inbox_id,
            "01".repeat(32)
        );
        assert_eq!(
            mutable.as_ref().unwrap().attributes["description"],
            "good description"
        );
    }

    /// `fixtures/generate.py` records every fixture hash. A corpus rebuild
    /// must update the record too. Native runs also check every other
    /// recorded fixture file.
    #[xmtp_common::test(unwrap_try = true)]
    fn corpus_hash_is_recorded() {
        use sha2::Digest;
        let hashes: Value = serde_json::from_str(include_str!("../fixtures/hashes.json"))?;
        let digest = hex::encode(sha2::Sha256::digest(CORPUS.as_bytes()));
        assert_eq!(hashes["legacy-context-corpus.json"], digest.as_str());
        #[cfg(not(target_arch = "wasm32"))]
        {
            let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures");
            let recorded = hashes.as_object().unwrap();
            assert!(recorded.len() > 1);
            let stale: Vec<_> = recorded
                .iter()
                .filter(|(name, hash)| {
                    let bytes = std::fs::read(fixtures.join(name.as_str()))
                        .unwrap_or_else(|error| panic!("cannot read fixture {name}: {error}"));
                    hex::encode(sha2::Sha256::digest(&bytes)) != hash.as_str().unwrap()
                })
                .map(|(name, _)| name.as_str())
                .collect();
            assert!(stale.is_empty(), "hashes.json is stale for {stale:?}");
        }
    }

    /// The reader ignores bytes after the context, as the legacy decoder did.
    // verifies: MIG-005
    #[xmtp_common::test(unwrap_try = true)]
    fn trailing_bytes_after_a_context_are_ignored() {
        let case = corpus()
            .into_iter()
            .find(|case| case.name == "everything")
            .unwrap();
        assert_ne!(case.expected["creator"], Value::Null);
        let mut bytes = case.context;
        bytes.extend([0xff; 3]);
        assert_eq!(summary(&super::decode(&bytes)), case.expected);
    }

    // verifies: MIG-005
    #[xmtp_common::test(unwrap_try = true)]
    fn unavailable_appdata_creator_keeps_legacy_creator() {
        let mut failures = vec![];
        for (name, expected) in [
            ("appdata-creator-absent", "02"),
            ("appdata-creator-malformed", "02"),
            ("appdata-creator-valid", "01"),
        ] {
            let (immutable, mutable) = super::decode(&context(name));
            if immutable
                .as_ref()
                .map(|value| value.creator_inbox_id.as_str())
                != Some(expected.repeat(32).as_str())
            {
                failures.push(format!("{name}: expected {expected}, got {immutable:?}"));
            }
            assert_eq!(mutable.unwrap().attributes["group_name"], "AppData Group");
        }
        assert!(
            failures.is_empty(),
            "available legacy creator was discarded: {failures:?}"
        );
    }

    // verifies: MIG-005
    #[xmtp_common::test(unwrap_try = true)]
    fn context_byte_budget_rejects_large_optional_metadata() {
        let below_limit = context("appdata-tree-hash-1024");
        let (immutable, mutable) = super::decode(&below_limit);
        assert_eq!(immutable.unwrap().creator_inbox_id, "01".repeat(32));
        let mutable = mutable.unwrap();
        assert_eq!(mutable.attributes["group_name"], "AppData Group");
        assert_eq!(mutable.attributes["description"], "good description");
        let bytes = with_tree_hash(&below_limit, super::MAX_CONTEXT_BYTES);
        assert!(bytes.len() > super::MAX_CONTEXT_BYTES);
        assert!(
            crate::legacy_context::LegacyContext::read(&bytes).is_some(),
            "the over-budget context is otherwise readable"
        );
        assert_eq!(super::decode(&bytes), (None, None));
    }

    // verifies: MIG-005
    #[xmtp_common::test(unwrap_try = true)]
    fn malformed_unrelated_immutable_field_keeps_creator() {
        let creator = field(3, b"known creator");
        let invalid_account = field(2, &[0xff]);
        for bytes in [
            [creator.clone(), invalid_account.clone()].concat(),
            [invalid_account, creator.clone()].concat(),
            [creator.clone(), vec![0x22, 0x7f, 0x01]].concat(),
        ] {
            assert!(super::GroupMetadataV1::decode(bytes.as_slice()).is_err());
            let result = super::decode_legacy_immutable(&bytes).unwrap();
            assert_eq!(result.creator_inbox_id, "known creator");
        }
        for invalid in [field(3, &[0xff]), field(3, &[])] {
            let bytes = [creator.clone(), invalid].concat();
            assert!(super::decode_legacy_immutable(&bytes).is_none());
        }
    }

    // verifies: MIG-005
    #[xmtp_common::test(unwrap_try = true)]
    fn malformed_legacy_attribute_keeps_other_fields() {
        let mut bytes = GroupMutableMetadataV1 {
            attributes: [
                ("group_name".into(), "Legacy Group".into()),
                ("description".into(), "old value".into()),
            ]
            .into(),
            admin_list: Some(Inboxes {
                inbox_ids: vec!["01".repeat(32)],
            }),
            super_admin_list: None,
        }
        .encode_to_vec();
        let mut invalid = field(1, b"description");
        invalid.extend(field(2, &[0xff]));
        bytes.extend(field(1, &invalid));
        bytes.extend(
            GroupMutableMetadataV1 {
                attributes: [("app_data".into(), "tail".into())].into(),
                super_admin_list: Some(Inboxes {
                    inbox_ids: vec!["02".repeat(32)],
                }),
                admin_list: None,
            }
            .encode_to_vec(),
        );
        assert!(GroupMutableMetadataV1::decode(bytes.as_slice()).is_err());
        let result = super::decode_legacy_mutable(&bytes).unwrap();
        assert_eq!(result.attributes["group_name"], "Legacy Group");
        assert_eq!(result.attributes["app_data"], "tail");
        assert!(!result.attributes.contains_key("description"));
        assert_eq!(result.admin_list, vec!["01".repeat(32)]);
        assert_eq!(result.super_admin_list, vec!["02".repeat(32)]);
    }

    // verifies: MIG-005
    #[xmtp_common::test(unwrap_try = true)]
    fn malformed_legacy_inbox_keeps_other_entries() {
        let mut bytes = GroupMutableMetadataV1 {
            attributes: [("group_name".into(), "Legacy Group".into())].into(),
            admin_list: None,
            super_admin_list: None,
        }
        .encode_to_vec();
        let mut admins = Inboxes {
            inbox_ids: vec!["01".repeat(32)],
        }
        .encode_to_vec();
        admins.extend(field(1, &[0xff]));
        admins.extend(
            Inboxes {
                inbox_ids: vec!["02".repeat(32)],
            }
            .encode_to_vec(),
        );
        bytes.extend(field(2, &admins));
        let result = super::decode_legacy_mutable(&bytes).unwrap();
        assert_eq!(result.attributes["group_name"], "Legacy Group");
        assert_eq!(result.admin_list, vec!["01".repeat(32), "02".repeat(32)]);
    }

    // verifies: MIG-005
    #[xmtp_common::test(unwrap_try = true)]
    fn truncated_legacy_field_keeps_prior_fields() {
        let mut bytes = GroupMutableMetadataV1 {
            attributes: [("group_name".into(), "Legacy Group".into())].into(),
            admin_list: Some(Inboxes {
                inbox_ids: vec!["01".repeat(32)],
            }),
            super_admin_list: None,
        }
        .encode_to_vec();
        bytes.extend([0x0a, 0xff]);
        let result = super::decode_legacy_mutable(&bytes).unwrap();
        assert_eq!(result.attributes["group_name"], "Legacy Group");
        assert_eq!(result.admin_list, vec!["01".repeat(32)]);
    }

    // verifies: MIG-005
    #[xmtp_common::test(unwrap_try = true)]
    fn malformed_appdata_component_keeps_independent_fields() {
        let (immutable, mutable) =
            super::decode(include_bytes!("../fixtures/appdata-context.bincode"));
        assert_eq!(immutable.unwrap().creator_inbox_id, "01".repeat(32));
        let mutable = mutable.unwrap();
        assert_eq!(mutable.attributes["group_name"], "AppData Group");
        assert_eq!(mutable.attributes["description"], "good description");
        assert_eq!(mutable.attributes["group_image_url_square"], "");
        assert_eq!(mutable.attributes["app_data"], "");
        assert!(!mutable.attributes.contains_key("message_disappear_from_ns"));
    }

    /// A legacy group context in the format `legacy_context.rs` documents,
    /// with an Unknown extension per `unknown` entry and an AppData dictionary
    /// when `app_data` is given.
    fn legacy_context(unknown: &[(u16, &[u8])], app_data: Option<&[(u16, &[u8])]>) -> Vec<u8> {
        fn bytes(out: &mut Vec<u8>, value: &[u8]) {
            out.extend((value.len() as u64).to_le_bytes());
            out.extend(value);
        }
        let mut out = vec![];
        out.extend(0u32.to_le_bytes()); // Mls10
        out.extend(1u32.to_le_bytes()); // ciphersuite
        bytes(&mut out, &[9; 16]); // group_id
        out.extend(3u64.to_le_bytes()); // epoch
        bytes(&mut out, &[]); // tree_hash
        bytes(&mut out, &[]); // confirmed_transcript_hash
        let count = unknown.len() + usize::from(app_data.is_some());
        out.extend((count as u64).to_le_bytes());
        for (id, value) in unknown {
            out.extend(7u32.to_le_bytes());
            out.extend(id.to_le_bytes());
            bytes(&mut out, value);
        }
        if let Some(entries) = app_data {
            out.extend(8u32.to_le_bytes());
            out.extend((entries.len() as u64).to_le_bytes());
            for (key, value) in entries {
                out.extend(key.to_le_bytes());
                out.extend(key.to_le_bytes());
                bytes(&mut out, value);
            }
        }
        out
    }

    /// Legacy groups stored the commit-log signer's private key in both places
    /// a converter reads mutable metadata. Neither copy may reach the archive.
    #[xmtp_common::test(unwrap_try = true)]
    fn legacy_commit_log_signer_is_not_exported() {
        let signer = [7u8; 32];
        let signer_hex = hex::encode(signer);
        let attributes = GroupMutableMetadataV1 {
            attributes: [
                ("group_name".into(), "Legacy Group".into()),
                ("_commit_log_signer".into(), signer_hex.clone()),
            ]
            .into(),
            admin_list: None,
            super_admin_list: None,
        }
        .encode_to_vec();
        let extension = legacy_context(&[(super::MUTABLE_METADATA_EXTENSION, &attributes)], None);
        let dictionary = legacy_context(
            &[],
            Some(&[
                (super::COMPONENT_REGISTRY, &[]),
                (0x8004, b"AppData Group"),
                (0x800B, &signer),
            ]),
        );
        for (context, name) in [(extension, "Legacy Group"), (dictionary, "AppData Group")] {
            let (_, mutable) = super::decode(&context);
            let mutable = mutable.unwrap();
            assert_eq!(mutable.attributes["group_name"], name);
            assert!(
                !mutable.attributes.contains_key("_commit_log_signer"),
                "the {name} context kept the signer"
            );
            assert!(
                !mutable
                    .attributes
                    .values()
                    .any(|value| *value == signer_hex)
            );
        }
    }
}
