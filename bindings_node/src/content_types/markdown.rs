use napi::bindgen_prelude::Result;
use napi_derive::napi;
use xmtp_content_types::{ContentCodec, markdown::MarkdownCodec};

use crate::ErrorWrapper;
use crate::encoded_content::{ContentTypeId, EncodedContent};

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
