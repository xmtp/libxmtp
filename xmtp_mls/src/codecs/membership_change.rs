use std::collections::HashMap;

use prost::Message;

use xmtp_proto::xmtp::mls::message_contents::{
    ContentTypeId, EncodedContent, GroupMembershipChanges,
};

use super::{CodecError, ContentCodec};

pub struct GroupMembershipChangeCodec {}

impl GroupMembershipChangeCodec {
    const AUTHORITY_ID: &'static str = "xmtp.org";
    const TYPE_ID: &'static str = "group_membership_change";
}

impl ContentCodec<GroupMembershipChanges> for GroupMembershipChangeCodec {
    fn content_type() -> ContentTypeId {
        ContentTypeId {
            authority_id: GroupMembershipChangeCodec::AUTHORITY_ID.to_string(),
            type_id: GroupMembershipChangeCodec::TYPE_ID.to_string(),
            version_major: 1,
            version_minor: 0,
        }
    }

    fn encode(data: GroupMembershipChanges) -> Result<EncodedContent, CodecError> {
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

    fn decode(content: EncodedContent) -> Result<GroupMembershipChanges, CodecError> {
        let decoded = GroupMembershipChanges::decode(content.content.as_slice())
            .map_err(|e| CodecError::Decode(e.to_string()))?;

        Ok(decoded)
    }
}

#[cfg(test)]
mod tests {
    use xmtp_proto::xmtp::mls::message_contents::MembershipChange;

    use crate::utils::test::{rand_string, rand_vec};

    use super::*;

    #[test]
    fn test_encode_decode() {
        let new_member = MembershipChange {
            installation_ids: vec![rand_vec()],
            account_address: rand_string(),
            initiated_by_account_address: "".to_string(),
        };
        let data = GroupMembershipChanges {
            members_added: vec![new_member.clone()],
            members_removed: vec![],
            installations_added: vec![],
            installations_removed: vec![],
        };

        let encoded = GroupMembershipChangeCodec::encode(data).unwrap();
        assert_eq!(
            encoded.clone().r#type.unwrap().type_id,
            "group_membership_change"
        );
        assert!(!encoded.content.is_empty());

        let decoded = GroupMembershipChangeCodec::decode(encoded).unwrap();
        assert_eq!(decoded.members_added[0], new_member);
    }
}
