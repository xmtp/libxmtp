use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone)]
pub struct Attachment {
  pub filename: Option<String>,
  #[wasm_bindgen(js_name = "mimeType")]
  pub mime_type: String,
  pub content: Vec<u8>,
}

impl From<xmtp_content_types::attachment::Attachment> for Attachment {
  fn from(attachment: xmtp_content_types::attachment::Attachment) -> Self {
    Self {
      filename: attachment.filename,
      mime_type: attachment.mime_type,
      content: attachment.content,
    }
  }
}
