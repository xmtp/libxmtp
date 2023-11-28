use std::collections::HashMap;

use prost::Message;
use xmtp_proto::xmtp::mls::message_contents::{
    ContentTypeId, EncodedContent, GroupMembershipChange,
};

use super::{CodecError, ContentCodec};

pub struct GroupMembershipChangeCodec {}

impl GroupMembershipChangeCodec {
    const AUTHORITY_ID: &'static str = "xmtp.org";
    const TYPE_ID: &'static str = "group_membership_change";
}

impl ContentCodec<GroupMembershipChange> for GroupMembershipChangeCodec {
    fn content_type() -> ContentTypeId {
        ContentTypeId {
            authority_id: GroupMembershipChangeCodec::AUTHORITY_ID.to_string(),
            type_id: GroupMembershipChangeCodec::TYPE_ID.to_string(),
            version_major: 1,
            version_minor: 0,
        }
    }

    fn encode(data: GroupMembershipChange) -> Result<EncodedContent, CodecError> {
        let mut buf = Vec::new();
        data.encode(&mut buf)
            .map_err(|e| CodecError::Encode(e.to_string()))?;

        Ok(EncodedContent {
            r#type: Some(GroupMembershipChangeCodec::content_type()),
            parameters: HashMap::new(),
            fallback: None,
            compression: None,
            content: buf,
        })
    }

    fn decode(content: EncodedContent) -> Result<GroupMembershipChange, CodecError> {
        let decoded = GroupMembershipChange::decode(content.content.as_slice())
            .map_err(|e| CodecError::Decode(e.to_string()))?;

        Ok(decoded)
    }
}
