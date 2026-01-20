use crate::ErrorWrapper;
use crate::messages::encoded_content::{ContentTypeId, EncodedContent};
use napi::bindgen_prelude::Result;
use napi_derive::napi;
use xmtp_content_types::{ContentCodec, markdown::MarkdownCodec};

#[napi]
pub fn content_type_markdown() -> ContentTypeId {
  MarkdownCodec::content_type().into()
}

#[napi]
pub fn encode_markdown(text: String) -> Result<EncodedContent> {
  Ok(
    MarkdownCodec::encode(text)
      .map_err(ErrorWrapper::from)?
      .into(),
  )
}
