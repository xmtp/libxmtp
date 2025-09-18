use std::collections::HashMap;

use prost::Message;

use xmtp_proto::xmtp::mls::message_contents::{ContentTypeId, EncodedContent, GroupUpdated};

use super::{CodecError, ContentCodec};

pub struct GroupUpdatedCodec {}

impl GroupUpdatedCodec {
    const AUTHORITY_ID: &'static str = "xmtp.org";
    pub const TYPE_ID: &'static str = "group_updated";
    pub const MAJOR_VERSION: u32 = 1;
    pub const MINOR_VERSION: u32 = 0;
}

impl ContentCodec<GroupUpdated> for GroupUpdatedCodec {
    fn content_type() -> ContentTypeId {
        ContentTypeId {
            authority_id: GroupUpdatedCodec::AUTHORITY_ID.to_string(),
            type_id: GroupUpdatedCodec::TYPE_ID.to_string(),
            version_major: GroupUpdatedCodec::MAJOR_VERSION,
            version_minor: GroupUpdatedCodec::MINOR_VERSION,
        }
    }

    fn encode(data: GroupUpdated) -> Result<EncodedContent, CodecError> {
        let mut buf = Vec::new();
        data.encode(&mut buf)
            .map_err(|e| CodecError::Encode(e.to_string()))?;

        Ok(EncodedContent {
            r#type: Some(GroupUpdatedCodec::content_type()),
            parameters: HashMap::new(),
            fallback: None,
            compression: None,
            content: buf,
        })
    }

    fn decode(content: EncodedContent) -> Result<GroupUpdated, CodecError> {
        let decoded = GroupUpdated::decode(content.content.as_slice())
            .map_err(|e| CodecError::Decode(e.to_string()))?;

        Ok(decoded)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use super::*;
    use xmtp_common::rand_string;
    use xmtp_proto::xmtp::mls::message_contents::{GroupUpdated, group_updated::Inbox};

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_encode_decode() {
        let new_member = Inbox {
            inbox_id: rand_string::<24>(),
        };
        let data = GroupUpdated {
            initiated_by_inbox_id: rand_string::<24>(),
            added_inboxes: vec![new_member.clone()],
            removed_inboxes: vec![],
            metadata_field_changes: vec![],
            left_inboxes: vec![],
        };

        let encoded = GroupUpdatedCodec::encode(data).unwrap();
        assert_eq!(encoded.clone().r#type.unwrap().type_id, "group_updated");
        assert!(!encoded.content.is_empty());

        let decoded = GroupUpdatedCodec::decode(encoded).unwrap();
        assert_eq!(decoded.added_inboxes[0], new_member);
    }
}
