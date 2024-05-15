use prost::Message;
use serde::Serialize;
use xmtp_mls::{
    codecs::{text::TextCodec, ContentCodec},
    groups::MlsGroup,
    storage::group_message::StoredGroupMessage,
};
use xmtp_proto::xmtp::mls::message_contents::EncodedContent;

#[derive(Serialize, Debug)]
pub struct SerializableGroupMetadata {
    creator_account_address: String,
    policy: String,
}

#[derive(Serialize, Debug)]
pub struct SerializableGroup {
    pub group_id: String,
    pub members: Vec<String>,
    pub metadata: SerializableGroupMetadata,
}

impl<'a> From<&'a MlsGroup> for SerializableGroup {
    fn from(group: &'a MlsGroup) -> Self {
        let group_id = hex::encode(group.group_id.clone());
        let members = group
            .members()
            .expect("could not load members")
            .into_iter()
            .map(|m| m.account_address)
            .collect::<Vec<String>>();

        let metadata = group.metadata().expect("could not load metadata");
        let permissions = group.permissions().expect("could not load permissions");

        Self {
            group_id,
            members,
            metadata: SerializableGroupMetadata {
                creator_account_address: metadata.creator_account_address.clone(),
                policy: permissions
                    .preconfigured_policy()
                    .expect("could not get policy")
                    .to_string(),
            },
        }
    }
}

#[derive(Serialize, Debug, Clone)]
pub struct SerializableMessage {
    sender_inbox_id: String,
    sent_at_ns: u64,
    message_text: Option<String>,
    // content_type: String
}

impl SerializableMessage {
    pub fn from_stored_message(msg: &StoredGroupMessage) -> Self {
        let maybe_text = maybe_get_text(msg);
        Self {
            sender_inbox_id: msg.sender_inbox_id.clone(),
            sent_at_ns: msg.sent_at_ns as u64,
            message_text: maybe_text,
        }
    }
}

pub fn maybe_get_text(msg: &StoredGroupMessage) -> Option<String> {
    let contents = msg.decrypted_message_bytes.clone();
    let Ok(encoded_content) = EncodedContent::decode(contents.as_slice()) else {
        return None;
    };
    let Ok(decoded) = TextCodec::decode(encoded_content) else {
        log::warn!("Skipping over unrecognized codec");
        return None;
    };
    Some(decoded)
}
