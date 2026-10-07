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

const MUTABLE_METADATA_EXTENSION: u16 = 0xff00;
const COMPONENT_REGISTRY: u16 = 0x8000;
const CREATOR: u16 = 0xbffe;
// Optional metadata has a fixed byte budget. Larger contexts become unknown.
const MAX_CONTEXT_BYTES: usize = 1024 * 1024;

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
    let mut immutable =
        extensions.immutable_metadata().and_then(|e| {
            match GroupMetadataV1::decode(e.metadata().as_slice()) {
                Ok(value) if !value.creator_inbox_id.is_empty() => Some(ImmutableMetadataSave {
                    creator_inbox_id: value.creator_inbox_id,
                }),
                _ => None,
            }
        });
    let mut mutable = extensions.iter().find_map(|ext| {
        if let Extension::Unknown(MUTABLE_METADATA_EXTENSION, UnknownExtension(bytes)) = ext {
            match GroupMutableMetadataV1::decode(bytes.as_slice()) {
                Ok(value) => Some(MutableMetadataSave {
                    attributes: value.attributes,
                    admin_list: value.admin_list.map(|v| v.inbox_ids).unwrap_or_default(),
                    super_admin_list: value
                        .super_admin_list
                        .map(|v| v.inbox_ids)
                        .unwrap_or_default(),
                }),
                Err(_) => None,
            }
        } else {
            None
        }
    });
    if let Some(extension) = extensions.app_data_dictionary() {
        let dict = extension.dictionary();
        if dict.get(&COMPONENT_REGISTRY).is_some() {
            immutable =
                dict.get(&CREATOR)
                    .and_then(|bytes| match InboxId::tls_deserialize_exact(bytes) {
                        Ok(id) => Some(ImmutableMetadataSave {
                            creator_inbox_id: hex::encode(id.as_bytes()),
                        }),
                        Err(_) => None,
                    });
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

#[cfg(test)]
mod tests {
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
