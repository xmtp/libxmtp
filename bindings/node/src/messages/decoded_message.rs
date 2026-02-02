use crate::content_types::decoded_message_content::DecodedMessageContent;
use crate::messages::encoded_content::ContentTypeId;
use crate::messages::{DeliveryStatus, GroupMessageKind};
use napi::Error;
use napi::bindgen_prelude::{BigInt, Result};
use napi_derive::napi;
use prost::Message;
use xmtp_mls::messages::decoded_message::{
  DecodedMessage as XmtpDecodedMessage, MessageBody, Reply as ProcessedReply,
};
use xmtp_proto::xmtp::mls::message_contents::EncodedContent;

#[derive(Clone)]
#[napi]
pub struct DecodedMessage {
  inner: Box<XmtpDecodedMessage>,
  /// The content to display (edited content if edited, otherwise original)
  content_body: MessageBody,
  pub id: String,
  sent_at_ns: BigInt,
  pub kind: GroupMessageKind,
  pub sender_installation_id: String,
  pub sender_inbox_id: String,
  content_type: ContentTypeId,
  pub conversation_id: String,
  pub fallback: Option<String>,
  pub delivery_status: DeliveryStatus,
  pub num_replies: i64,
  expires_at_ns: Option<BigInt>,
  edited_at_ns: Option<BigInt>,
}

#[napi]
impl DecodedMessage {
  #[napi(getter)]
  pub fn sent_at_ns(&self) -> BigInt {
    self.sent_at_ns.clone()
  }

  #[napi(getter)]
  pub fn expires_at_ns(&self) -> Option<BigInt> {
    self.expires_at_ns.clone()
  }

  #[napi(getter)]
  pub fn edited_at_ns(&self) -> Option<BigInt> {
    self.edited_at_ns.clone()
  }

  #[napi(getter)]
  pub fn is_edited(&self) -> bool {
    self.edited_at_ns.is_some()
  }

  #[napi(getter)]
  pub fn reactions(&self) -> Result<Vec<DecodedMessage>> {
    self
      .inner
      .reactions
      .iter()
      .map(|r| r.clone().try_into())
      .collect()
  }

  #[napi(getter)]
  pub fn content_type(&self) -> ContentTypeId {
    self.content_type.clone()
  }

  #[napi(getter)]
  pub fn content(&self) -> Result<DecodedMessageContent> {
    self.content_body.clone().try_into()
  }
}

impl TryFrom<XmtpDecodedMessage> for DecodedMessage {
  type Error = Error;

  fn try_from(msg: XmtpDecodedMessage) -> Result<Self> {
    let edited_at_ns = msg.edited.as_ref().map(|e| BigInt::from(e.edited_at_ns));
    let original_content_type: ContentTypeId = msg.metadata.content_type.clone().into();

    // If the message has been edited, use the edited content instead of the original
    let (content_body, content_type) = if let Some(edited) = &msg.edited {
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
                // Keep the original content type since we're preserving the reply structure
                final_content_type = original_content_type.clone();
              }
              (edited_body, final_content_type)
            }
            Err(_) => (msg.content.clone(), original_content_type.clone()),
          }
        }
        Err(_) => (msg.content.clone(), original_content_type.clone()),
      }
    } else {
      (msg.content.clone(), original_content_type.clone())
    };

    Ok(Self {
      id: hex::encode(&msg.metadata.id),
      sent_at_ns: BigInt::from(msg.metadata.sent_at_ns),
      kind: msg.metadata.kind.into(),
      sender_installation_id: hex::encode(&msg.metadata.sender_installation_id),
      sender_inbox_id: msg.metadata.sender_inbox_id.clone(),
      content_type,
      conversation_id: hex::encode(&msg.metadata.group_id),
      fallback: msg.fallback_text.clone(),
      delivery_status: msg.metadata.delivery_status.into(),
      num_replies: msg.num_replies as i64,
      expires_at_ns: msg.metadata.expires_at_ns.map(BigInt::from),
      edited_at_ns,
      content_body,
      inner: Box::new(msg),
    })
  }
}
