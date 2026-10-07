//! Legacy metadata encodings. OpenMLS and bincode revisions are pinned by the
//! workspace and checked against library-produced database fixtures.
use bincode::Options;
use openmls::{
    extensions::{Extension, UnknownExtension},
    prelude::GroupContext,
};
use prost::Message;
use tls_codec::Deserialize;
use xmtp_mls_common::{
    group_mutable_metadata::{GroupMutableMetadata, merge_dict_into_mutable_metadata_lossy},
    inbox_id::InboxId,
};
use xmtp_proto::xmtp::{
    device_sync::group_backup::{ImmutableMetadataSave, MutableMetadataSave},
    mls::message_contents::{GroupMetadataV1, GroupMutableMetadataV1},
};

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
    let context: GroupContext = match bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .with_little_endian()
        .allow_trailing_bytes()
        .with_limit(MAX_CONTEXT_BYTES as u64)
        .deserialize(bytes)
    {
        Ok(context) => context,
        Err(_) => return (None, None),
    };
    let extensions = context.extensions();
    let mut immutable = extensions
        .immutable_metadata()
        .and_then(|e| decode_legacy_immutable(e.metadata().as_slice()));
    let mut mutable = extensions.iter().find_map(|ext| {
        if let Extension::Unknown(MUTABLE_METADATA_EXTENSION, UnknownExtension(bytes)) = ext {
            decode_legacy_mutable(bytes)
        } else {
            None
        }
    });
    if let Some(extension) = extensions.app_data_dictionary() {
        let dict = extension.dictionary();
        if dict.get(&COMPONENT_REGISTRY).is_some() {
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
            let errors = merge_dict_into_mutable_metadata_lossy(&mut result, extensions);
            if !errors.is_empty() {
                tracing::debug!(
                    count = errors.len(),
                    "Cannot decode optional legacy metadata fields"
                );
            }
            mutable = Some(MutableMetadataSave {
                attributes: result.attributes,
                admin_list: result.admin_list,
                super_admin_list: result.super_admin_list,
            });
        }
    }
    (immutable, mutable)
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

    if let Ok(value) = GroupMutableMetadataV1::decode(bytes) {
        return Some(MutableMetadataSave {
            attributes: value.attributes,
            admin_list: value.admin_list.map(|v| v.inbox_ids).unwrap_or_default(),
            super_admin_list: value
                .super_admin_list
                .map(|v| v.inbox_ids)
                .unwrap_or_default(),
        });
    }
    let mut result = MutableMetadataSave::default();
    let mut recovered = false;
    protobuf_fields(bytes, |tag, raw, payload| {
        match tag {
            1 => match GroupMutableMetadataV1::decode(raw) {
                Ok(value) => {
                    result.attributes.extend(value.attributes);
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
    use prost::Message;
    use xmtp_proto::xmtp::mls::message_contents::{GroupMutableMetadataV1, Inboxes};

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
    fn unavailable_appdata_creator_keeps_legacy_creator() {
        use openmls::extensions::{AppDataDictionaryExtension, Extension, Metadata};
        let context: super::GroupContext =
            bincode::deserialize(include_bytes!("../fixtures/appdata-context.bincode"))?;
        let original = context
            .extensions()
            .app_data_dictionary()
            .unwrap()
            .dictionary();
        let valid = original.get(&super::CREATOR).unwrap().to_vec();
        let mut failures = vec![];
        for (preferred, expected) in [(None, "02"), (Some(vec![0xff]), "02"), (Some(valid), "01")] {
            let mut dictionary = original.clone();
            dictionary.remove(&super::CREATOR);
            if let Some(bytes) = preferred {
                dictionary.insert(super::CREATOR, bytes);
            }
            let mut extensions = context.extensions().clone();
            extensions.add_or_replace(Extension::ImmutableMetadata(Metadata::new(
                super::GroupMetadataV1 {
                    creator_inbox_id: "02".repeat(32),
                    ..Default::default()
                }
                .encode_to_vec(),
            )))?;
            extensions.add_or_replace(Extension::AppDataDictionary(
                AppDataDictionaryExtension::new(dictionary),
            ))?;
            let bytes = bincode::serialize(&(
                context.protocol_version(),
                context.ciphersuite(),
                context.group_id(),
                context.epoch(),
                context.tree_hash(),
                context.confirmed_transcript_hash(),
                extensions,
            ))?;
            let (immutable, mutable) = super::decode(&bytes);
            if immutable
                .as_ref()
                .map(|value| value.creator_inbox_id.as_str())
                != Some(expected.repeat(32).as_str())
            {
                failures.push(format!("expected {expected}, got {immutable:?}"));
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
            ..Default::default()
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
                ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
        }
        .encode_to_vec();
        bytes.extend([0x0a, 0xff]);
        let result = super::decode_legacy_mutable(&bytes).unwrap();
        assert_eq!(result.attributes["group_name"], "Legacy Group");
        assert_eq!(result.admin_list, vec!["01".repeat(32)]);
    }

    // verifies: MIG-005
    #[xmtp_common::test(unwrap_try = true)]
    fn absent_appdata_fields_keep_legacy_defaults() {
        let (_, mutable) = super::decode(include_bytes!("../fixtures/appdata-context.bincode"));
        let mutable = mutable.unwrap();
        assert_eq!(mutable.attributes["group_image_url_square"], "");
        assert_eq!(mutable.attributes["app_data"], "");
        assert!(!mutable.attributes.contains_key("message_disappear_from_ns"));
    }

    // verifies: MIG-005
    #[xmtp_common::test(unwrap_try = true)]
    fn context_byte_budget_rejects_large_optional_metadata() {
        let context: super::GroupContext =
            bincode::deserialize(include_bytes!("../fixtures/appdata-context.bincode"))?;
        let expected = super::decode(include_bytes!("../fixtures/appdata-context.bincode"));
        let below_limit = bincode::serialize(&(
            context.protocol_version(),
            context.ciphersuite(),
            context.group_id(),
            context.epoch(),
            vec![0u8; 1024],
            context.confirmed_transcript_hash(),
            context.extensions(),
        ))?;
        assert_eq!(super::decode(&below_limit), expected);
        let bytes = bincode::serialize(&(
            context.protocol_version(),
            context.ciphersuite(),
            context.group_id(),
            context.epoch(),
            vec![0u8; 1024 * 1024],
            context.confirmed_transcript_hash(),
            context.extensions(),
        ))?;
        assert!(bincode::deserialize::<super::GroupContext>(&bytes).is_ok());
        assert_eq!(super::decode(&bytes), (None, None));
    }

    // verifies: MIG-005
    #[xmtp_common::test(unwrap_try = true)]
    fn oversized_declared_context_lengths_degrade_without_panicking() {
        let context: super::GroupContext =
            bincode::deserialize(include_bytes!("../fixtures/appdata-context.bincode"))?;
        let prefix = bincode::serialize(&(
            context.protocol_version(),
            context.ciphersuite(),
            context.group_id(),
            context.epoch(),
        ))?;
        for length in [1u64 << 40, u64::MAX] {
            let mut bytes = prefix.clone();
            bytes.extend_from_slice(&length.to_le_bytes());
            bytes.push(0);
            assert_eq!(super::decode(&bytes), (None, None));
        }
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
        assert!(!mutable.attributes.contains_key("message_disappear_from_ns"));
    }
}
