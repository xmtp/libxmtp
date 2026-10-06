//! Legacy metadata encodings. OpenMLS and bincode revisions are pinned by the
//! workspace and checked against library-produced database fixtures.
use openmls::{
    extensions::{Extension, UnknownExtension},
    prelude::GroupContext,
};
use prost::Message;
use tls_codec::Deserialize;
use xmtp_mls_common::{inbox_id::InboxId, tls_set::TlsSet};
use xmtp_proto::xmtp::{
    device_sync::group_backup::{ImmutableMetadataSave, MutableMetadataSave},
    mls::message_contents::{GroupMetadataV1, GroupMutableMetadataV1},
};

const MUTABLE_METADATA_EXTENSION: u16 = 0xff00;
const COMPONENT_REGISTRY: u16 = 0x8000;
const CREATOR: u16 = 0xbffe;
const ADMIN: u16 = 0x8002;
const SUPER_ADMIN: u16 = 0x8001;
const ATTRIBUTES: &[(u16, &str)] = &[
    (0x8004, "group_name"),
    (0x8005, "description"),
    (0x8006, "group_image_url_square"),
    (0x8007, "message_disappear_from_ns"),
    (0x8008, "message_disappear_in_ns"),
    (0x8009, "app_data"),
    (0x800a, "minimum_supported_protocol_version"),
    (0x800b, "commit_log_signer"),
];

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
    let context: GroupContext = match bincode::deserialize(bytes) {
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
            let mut result = MutableMetadataSave::default();
            for &(id, name) in ATTRIBUTES {
                if let Some(bytes) = dict.get(&id) {
                    let value = match id {
                        0x8007 | 0x8008 => match <[u8; 8]>::try_from(bytes) {
                            Ok(bytes) => Some(i64::from_be_bytes(bytes).to_string()),
                            Err(_) => None,
                        },
                        0x800b if bytes.len() == 32 => Some(hex::encode(bytes)),
                        0x800b => None,
                        _ => match String::from_utf8(bytes.to_vec()) {
                            Ok(value) => Some(value),
                            Err(error) => {
                                tracing::debug!(component = id, %error, "Cannot decode optional legacy metadata");
                                None
                            }
                        },
                    };
                    if let Some(value) = value {
                        result.attributes.insert(name.to_owned(), value);
                    }
                }
            }
            for (id, list) in [
                (ADMIN, &mut result.admin_list),
                (SUPER_ADMIN, &mut result.super_admin_list),
            ] {
                if let Some(bytes) = dict.get(&id)
                    && let Ok(ids) = TlsSet::<InboxId>::tls_deserialize_exact(bytes)
                {
                    *list = ids.iter().map(|id| hex::encode(id.as_bytes())).collect();
                }
            }
            mutable = Some(result);
        }
    }
    (immutable, mutable)
}

#[cfg(test)]
mod tests {
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
