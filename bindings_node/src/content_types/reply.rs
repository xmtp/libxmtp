use crate::ErrorWrapper;
use crate::encoded_content::{ContentTypeId, EncodedContent};
use crate::enriched_message::DecodedMessage;
use napi::bindgen_prelude::Result;
use napi_derive::napi;
use xmtp_content_types::ContentCodec;
use xmtp_content_types::reply::ReplyCodec as XmtpReplyCodec;

use super::decoded_message_body::DecodedMessageBody;
use xmtp_mls::messages::decoded_message::DecodedMessage as RustDecodedMessage;

#[derive(Clone)]
#[napi]
pub struct EnrichedReply {
  in_reply_to: Option<Box<RustDecodedMessage>>,
  content: DecodedMessageBody,
  reference_id: String,
}

#[napi]
impl EnrichedReply {
  #[napi(getter)]
  pub fn reference_id(&self) -> String {
    self.reference_id.clone()
  }

  #[napi(getter)]
  pub fn content(&self) -> DecodedMessageBody {
    self.content.clone()
  }

  #[napi(getter)]
  pub fn in_reply_to(&self) -> Option<DecodedMessage> {
    self.in_reply_to.clone().map(|m| (*m).into())
  }
}

impl From<xmtp_mls::messages::decoded_message::Reply> for EnrichedReply {
  fn from(reply: xmtp_mls::messages::decoded_message::Reply) -> Self {
    Self {
      in_reply_to: reply.in_reply_to,
      content: reply.content.as_ref().clone().into(),
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
pub struct ReplyCodec {}

#[napi]
impl ReplyCodec {
  #[napi]
  pub fn content_type() -> ContentTypeId {
    XmtpReplyCodec::content_type().into()
  }

  #[napi]
  pub fn encode(reply: Reply) -> Result<EncodedContent> {
    let encoded_content = XmtpReplyCodec::encode(reply.into()).map_err(ErrorWrapper::from)?;
    Ok(encoded_content.into())
  }

  #[napi]
  pub fn decode(encoded_content: EncodedContent) -> Result<Reply> {
    Ok(
      XmtpReplyCodec::decode(encoded_content.into())
        .map(Into::into)
        .map_err(ErrorWrapper::from)?,
    )
  }

  #[napi]
  pub fn should_push() -> bool {
    XmtpReplyCodec::should_push()
  }
}
