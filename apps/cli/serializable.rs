use prost::Message;
use serde::Serialize;
use valuable::Valuable;
use xmtp_content_types::{ContentCodec, text::TextCodec};
use xmtp_db::group_message::StoredGroupMessage;
use xmtp_mls::{context::XmtpSharedContext, groups::MlsGroup};
use xmtp_proto::xmtp::mls::message_contents::EncodedContent;

#[derive(Serialize, Debug, Valuable)]
pub struct SerializableGroupMetadata {
    creator_inbox_id: String,
    policy: String,
}

#[derive(Serialize, Debug, Valuable)]
pub struct SerializableGroup {
    pub group_id: String,
    pub members: Vec<String>,
    pub metadata: SerializableGroupMetadata,
}

impl SerializableGroup {
    pub async fn from<Context: XmtpSharedContext>(group: &MlsGroup<Context>) -> Self {
        let group_id = hex::encode(group.group_id.clone());
        let members = group
            .members()
            .await
            .expect("could not load members")
            .into_iter()
            .map(|m| m.inbox_id)
            .collect::<Vec<String>>();

        let metadata = group.metadata().await.unwrap();
        let permissions = group.permissions().expect("could not load permissions");

        Self {
            group_id,
            members,
            metadata: SerializableGroupMetadata {
                creator_inbox_id: metadata.creator_inbox_id.clone(),
                policy: permissions
                    .preconfigured_policy()
                    .expect("could not get policy")
                    .to_string(),
            },
        }
    }
}

#[derive(Serialize, Debug, Clone, Valuable)]
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
        warn!("Skipping over unrecognized codec");
        return None;
    };
    Some(decoded)
}
