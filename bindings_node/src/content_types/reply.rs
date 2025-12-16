use crate::encoded_content::{ContentTypeId, EncodedContent};
use crate::enriched_message::DecodedMessage;
use napi::bindgen_prelude::Result;
use napi_derive::napi;
use xmtp_content_types::ContentCodec;
use xmtp_content_types::reply::ReplyCodec;

use super::decoded_message_content::DecodedMessageContent;
use xmtp_mls::messages::decoded_message::DecodedMessage as RustDecodedMessage;

#[derive(Clone)]
#[napi]
pub struct EnrichedReply {
  in_reply_to: Option<Box<RustDecodedMessage>>,
  content: Box<xmtp_mls::messages::decoded_message::MessageBody>,
  reference_id: String,
}

#[napi]
impl EnrichedReply {
  #[napi(getter)]
  pub fn reference_id(&self) -> String {
    self.reference_id.clone()
  }

  #[napi(getter)]
  pub fn content(&self) -> Result<DecodedMessageContent> {
    self.content.as_ref().clone().try_into()
  }

  #[napi(getter)]
  pub fn in_reply_to(&self) -> Result<Option<DecodedMessage>> {
    self
      .in_reply_to
      .clone()
      .map(|m| (*m).try_into())
      .transpose()
  }
}

impl From<xmtp_mls::messages::decoded_message::Reply> for EnrichedReply {
  fn from(reply: xmtp_mls::messages::decoded_message::Reply) -> Self {
    Self {
      in_reply_to: reply.in_reply_to,
      content: reply.content,
      reference_id: reply.reference_id,
    }
  }
}

#[derive(Clone)]
#[napi(object)]
pub struct Reply {
  pub content: EncodedContent,
  pub reference: String,
  pub reference_inbox_id: Option<String>,
}

impl From<xmtp_content_types::reply::Reply> for Reply {
  fn from(reply: xmtp_content_types::reply::Reply) -> Self {
    Self {
      content: reply.content.into(),
      reference: reply.reference,
      reference_inbox_id: reply.reference_inbox_id,
    }
  }
}

impl From<Reply> for xmtp_content_types::reply::Reply {
  fn from(reply: Reply) -> Self {
    Self {
      content: reply.content.into(),
      reference: reply.reference,
      reference_inbox_id: reply.reference_inbox_id,
    }
  }
}

#[napi]
pub fn reply_content_type() -> ContentTypeId {
  ReplyCodec::content_type().into()
}
