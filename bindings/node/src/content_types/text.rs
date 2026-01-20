use crate::ErrorWrapper;
use crate::messages::encoded_content::{ContentTypeId, EncodedContent};
use napi::bindgen_prelude::Result;
use napi_derive::napi;
use xmtp_content_types::{ContentCodec, text::TextCodec};

#[napi]
pub fn content_type_text() -> ContentTypeId {
  TextCodec::content_type().into()
}

#[napi]
pub fn encode_text(text: String) -> Result<EncodedContent> {
  Ok(TextCodec::encode(text).map_err(ErrorWrapper::from)?.into())
}
