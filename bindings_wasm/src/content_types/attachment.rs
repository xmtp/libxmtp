use crate::error::{ErrorCode, WasmError};
use js_sys::Uint8Array;
use prost::Message;
use wasm_bindgen::prelude::wasm_bindgen;
use xmtp_content_types::ContentCodec;
use xmtp_content_types::attachment::AttachmentCodec;
use xmtp_proto::xmtp::mls::message_contents::EncodedContent;

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
pub fn encode_attachment(attachment: Attachment) -> Result<Uint8Array, WasmError> {
  // Use AttachmentCodec to encode the attachment
  let encoded = AttachmentCodec::encode(attachment.into())
    .map_err(|e| WasmError::from_error(ErrorCode::ContentType, e))?;

  // Encode the EncodedContent to bytes
  let mut buf = Vec::new();
  encoded
    .encode(&mut buf)
    .map_err(|e| WasmError::from_error(ErrorCode::Encoding, e))?;

  Ok(Uint8Array::from(buf.as_slice()))
}

#[wasm_bindgen(js_name = "decodeAttachment")]
pub fn decode_attachment(bytes: Uint8Array) -> Result<Attachment, WasmError> {
  // Decode bytes into EncodedContent
  let encoded_content = EncodedContent::decode(bytes.to_vec().as_slice())
    .map_err(|e| WasmError::from_error(ErrorCode::Encoding, e))?;

  // Use AttachmentCodec to decode into Attachment and convert to Attachment
  AttachmentCodec::decode(encoded_content)
    .map(Into::into)
    .map_err(|e| WasmError::from_error(ErrorCode::ContentType, e))
}
