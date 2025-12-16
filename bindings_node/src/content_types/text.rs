use napi::bindgen_prelude::Result;
use napi_derive::napi;
use xmtp_content_types::{ContentCodec, text::TextCodec};

use crate::ErrorWrapper;
use crate::encoded_content::{ContentTypeId, EncodedContent};

#[napi]
pub fn text_content_type() -> ContentTypeId {
  TextCodec::content_type().into()
}

#[napi]
pub fn encode_text(text: String) -> Result<EncodedContent> {
  Ok(TextCodec::encode(text).map_err(ErrorWrapper::from)?.into())
}
