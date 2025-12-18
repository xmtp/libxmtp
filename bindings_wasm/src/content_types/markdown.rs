use crate::encoded_content::{ContentTypeId, EncodedContent};
use serde::{Deserialize, Serialize};
use tsify::Tsify;
use wasm_bindgen::JsError;
use wasm_bindgen::prelude::wasm_bindgen;
use xmtp_content_types::ContentCodec;
use xmtp_content_types::markdown::MarkdownCodec;

#[derive(Clone, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct MarkdownContent {
  pub content: String,
}

impl From<xmtp_mls::messages::decoded_message::Markdown> for MarkdownContent {
  fn from(markdown: xmtp_mls::messages::decoded_message::Markdown) -> Self {
    Self {
      content: markdown.content,
    }
  }
}

#[wasm_bindgen(js_name = "markdownContentType")]
pub fn markdown_content_type() -> ContentTypeId {
  MarkdownCodec::content_type().into()
}

#[wasm_bindgen(js_name = "encodeMarkdown")]
pub fn encode_markdown(text: String) -> Result<EncodedContent, JsError> {
  Ok(
    MarkdownCodec::encode(text)
      .map_err(|e| JsError::new(&format!("{}", e)))?
      .into(),
  )
}
