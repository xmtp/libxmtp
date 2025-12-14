use napi::bindgen_prelude::Result;
use napi_derive::napi;
use xmtp_content_types::{ContentCodec, text::TextCodec as XmtpTextCodec};

use crate::{
  ErrorWrapper,
  encoded_content::{ContentTypeId, EncodedContent},
};

#[derive(Clone)]
#[napi(object)]
pub struct TextContent {
  pub content: String,
}

impl From<xmtp_mls::messages::decoded_message::Text> for TextContent {
  fn from(text: xmtp_mls::messages::decoded_message::Text) -> Self {
    Self {
      content: text.content,
    }
  }
}

#[napi]
pub struct TextCodec {}

#[napi]
impl TextCodec {
  #[napi]
  pub fn content_type() -> ContentTypeId {
    XmtpTextCodec::content_type().into()
  }

  #[napi]
  pub fn encode(text: String) -> Result<EncodedContent> {
    let encoded_content = XmtpTextCodec::encode(text).map_err(ErrorWrapper::from)?;
    Ok(encoded_content.into())
  }

  #[napi]
  pub fn decode(encoded_content: EncodedContent) -> Result<String> {
    Ok(XmtpTextCodec::decode(encoded_content.into()).map_err(ErrorWrapper::from)?)
  }

  #[napi]
  pub fn should_push() -> bool {
    XmtpTextCodec::should_push()
  }
}
