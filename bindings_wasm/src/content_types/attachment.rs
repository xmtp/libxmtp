use crate::encoded_content::EncodedContent;
use wasm_bindgen::{JsError, prelude::wasm_bindgen};
use xmtp_content_types::ContentCodec;
use xmtp_content_types::attachment::AttachmentCodec;

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

impl From<Attachment> for xmtp_content_types::attachment::Attachment {
  fn from(attachment: Attachment) -> Self {
    Self {
      filename: attachment.filename,
      mime_type: attachment.mime_type,
      content: attachment.content,
    }
  }
}

#[wasm_bindgen(js_name = "encodeAttachment")]
pub fn encode_attachment(attachment: Attachment) -> Result<EncodedContent, JsError> {
  let encoded_content =
    AttachmentCodec::encode(attachment.into()).map_err(|e| JsError::new(&format!("{}", e)))?;
  Ok(encoded_content.into())
}

#[wasm_bindgen(js_name = "decodeAttachment")]
pub fn decode_attachment(encoded_content: EncodedContent) -> Result<Attachment, JsError> {
  // Use AttachmentCodec to decode into Attachment and convert to Attachment
  AttachmentCodec::decode(encoded_content.into())
    .map(Into::into)
    .map_err(|e| JsError::new(&format!("{}", e)))
}
