use prost::Message;
use serde::{Deserialize, Serialize};
use tsify::Tsify;
use wasm_bindgen::JsError;
use xmtp_mls::messages::decoded_message::{
  DecodedMessage as XmtpDecodedMessage, MessageBody, Reply as ProcessedReply,
};
use xmtp_proto::xmtp::mls::message_contents::EncodedContent;

use crate::content_types::decoded_message_content::DecodedMessageContent;
use crate::encoded_content::ContentTypeId;
use crate::messages::{DeliveryStatus, GroupMessageKind};

#[derive(Clone, Serialize, Deserialize, Tsify)]
#[tsify(
  into_wasm_abi,
  from_wasm_abi,
  large_number_types_as_bigints,
  hashmap_as_object
)]
#[serde(rename_all = "camelCase")]
pub struct DecodedMessage {
  pub id: String,
  pub sent_at_ns: i64,
  pub kind: GroupMessageKind,
  pub sender_installation_id: String,
  pub sender_inbox_id: String,
  pub content_type: ContentTypeId,
  pub conversation_id: String,
  pub content: DecodedMessageContent,
  #[serde(skip_serializing_if = "Option::is_none")]
  #[tsify(optional)]
  pub fallback: Option<String>,
  pub reactions: Vec<DecodedMessage>,
  pub delivery_status: DeliveryStatus,
  pub num_replies: i64,
  pub expires_at_ns: Option<i64>,
  #[serde(skip_serializing_if = "Option::is_none")]
  #[tsify(optional)]
  pub edited_at_ns: Option<i64>,
}

impl TryFrom<XmtpDecodedMessage> for DecodedMessage {
  type Error = JsError;

  fn try_from(msg: XmtpDecodedMessage) -> Result<Self, Self::Error> {
    let reactions: Result<Vec<_>, _> = msg.reactions.into_iter().map(|r| r.try_into()).collect();
    let edited_at_ns = msg.edited.as_ref().map(|e| e.edited_at_ns);
    let original_content_type: ContentTypeId = msg.metadata.content_type.clone().into();

    // If the message has been edited, use the edited content instead of the original
    let (content, content_type) = if let Some(edited) = &msg.edited {
      match EncodedContent::decode(&mut edited.content.as_slice()) {
        Ok(encoded_content) => {
          let edited_content_type: ContentTypeId = encoded_content
            .r#type
            .clone()
            .map(|ct| ct.into())
            .unwrap_or_else(|| original_content_type.clone());

          match MessageBody::try_from(encoded_content) {
            Ok(mut edited_body) => {
              let mut final_content_type = edited_content_type;

              // If both original and edited content are Replies, preserve in_reply_to
              if let (MessageBody::Reply(original_reply), MessageBody::Reply(edited_reply)) =
                (&msg.content, &mut edited_body)
              {
                edited_reply.in_reply_to = original_reply.in_reply_to.clone();
              }
              // If original is a Reply but edited content is Text, wrap text in Reply
              else if let MessageBody::Reply(original_reply) = &msg.content
                && let MessageBody::Text(text) = edited_body
              {
                edited_body = MessageBody::Reply(ProcessedReply {
                  in_reply_to: original_reply.in_reply_to.clone(),
                  content: Box::new(MessageBody::Text(text)),
                  reference_id: original_reply.reference_id.clone(),
                });
                final_content_type = original_content_type.clone();
              }
              match edited_body.try_into() {
                Ok(c) => (c, final_content_type),
                Err(_) => (msg.content.try_into()?, original_content_type.clone()),
              }
            }
            Err(_) => (msg.content.try_into()?, original_content_type.clone()),
          }
        }
        Err(_) => (msg.content.try_into()?, original_content_type.clone()),
      }
    } else {
      (msg.content.try_into()?, original_content_type.clone())
    };

    Ok(Self {
      id: hex::encode(msg.metadata.id),
      sent_at_ns: msg.metadata.sent_at_ns,
      kind: msg.metadata.kind.into(),
      sender_installation_id: hex::encode(msg.metadata.sender_installation_id),
      sender_inbox_id: msg.metadata.sender_inbox_id,
      content_type,
      conversation_id: hex::encode(msg.metadata.group_id),
      content,
      fallback: msg.fallback_text,
      reactions: reactions?,
      delivery_status: msg.metadata.delivery_status.into(),
      num_replies: msg.num_replies as i64,
      expires_at_ns: msg.metadata.expires_at_ns,
      edited_at_ns,
    })
  }
}
