use crate::encoded_content::EncodedContent;
use js_sys::Uint8Array;
use prost::Message;
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
pub fn encode_attachment(attachment: Attachment) -> Result<Uint8Array, JsError> {
  // Use AttachmentCodec to encode the attachment
  let encoded =
    AttachmentCodec::encode(attachment.into()).map_err(|e| JsError::new(&format!("{}", e)))?;

  // Encode the EncodedContent to bytes
  let mut buf = Vec::new();
  encoded
    .encode(&mut buf)
    .map_err(|e| JsError::new(&format!("{}", e)))?;

  Ok(Uint8Array::from(buf.as_slice()))
}

#[wasm_bindgen(js_name = "decodeAttachment")]
pub fn decode_attachment(encoded_content: EncodedContent) -> Result<Attachment, JsError> {
  // Use AttachmentCodec to decode into Attachment and convert to Attachment
  AttachmentCodec::decode(encoded_content.into())
    .map(Into::into)
    .map_err(|e| JsError::new(&format!("{}", e)))
}
