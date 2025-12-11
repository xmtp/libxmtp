use napi::bindgen_prelude::{Result, Uint8Array};
use napi_derive::napi;
use prost::Message;
use xmtp_content_types::{ContentCodec, text::TextCodec};

use crate::{ErrorWrapper, encoded_content::EncodedContent};

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
pub fn encode_text(text: String) -> Result<Uint8Array> {
  // Use TextCodec to encode the text
  let encoded = TextCodec::encode(text).map_err(ErrorWrapper::from)?;

  // Encode the EncodedContent to bytes
  let mut buf = Vec::new();
  encoded.encode(&mut buf).map_err(ErrorWrapper::from)?;

  Ok(buf.into())
}

#[napi]
pub fn decode_text(encoded_content: EncodedContent) -> Result<String> {
  // Use TextCodec to decode into String
  Ok(TextCodec::decode(encoded_content.into()).map_err(ErrorWrapper::from)?)
}
