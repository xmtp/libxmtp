use crate::error::{ErrorCode, WasmError};
use js_sys::Uint8Array;
use prost::Message;
use std::convert::TryFrom;
use wasm_bindgen::prelude::wasm_bindgen;
use xmtp_content_types::ContentCodec;
use xmtp_content_types::remote_attachment::RemoteAttachmentCodec;
use xmtp_proto::xmtp::mls::message_contents::EncodedContent;

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone)]
pub struct RemoteAttachment {
  pub url: String,
  #[wasm_bindgen(js_name = "contentDigest")]
  pub content_digest: String,
  pub secret: Vec<u8>,
  pub salt: Vec<u8>,
  pub nonce: Vec<u8>,
  pub scheme: String,
  #[wasm_bindgen(js_name = "contentLength")]
  pub content_length: u32,
  pub filename: Option<String>,
}

impl TryFrom<xmtp_content_types::remote_attachment::RemoteAttachment> for RemoteAttachment {
  type Error = WasmError;

  fn try_from(
    remote: xmtp_content_types::remote_attachment::RemoteAttachment,
  ) -> std::result::Result<Self, Self::Error> {
    let content_length = u32::try_from(remote.content_length).map_err(|_| {
      WasmError::content_type(format!(
        "content_length {} exceeds maximum value of {} bytes",
        remote.content_length,
        u32::MAX
      ))
    })?;

    Ok(Self {
      url: remote.url,
      content_digest: remote.content_digest,
      secret: remote.secret,
      salt: remote.salt,
      nonce: remote.nonce,
      scheme: remote.scheme,
      content_length,
      filename: remote.filename,
    })
  }
}

impl From<RemoteAttachment> for xmtp_content_types::remote_attachment::RemoteAttachment {
  fn from(remote: RemoteAttachment) -> Self {
    Self {
      url: remote.url,
      content_digest: remote.content_digest,
      secret: remote.secret,
      salt: remote.salt,
      nonce: remote.nonce,
      scheme: remote.scheme,
      content_length: remote.content_length as usize,
      filename: remote.filename,
    }
  }
}

#[wasm_bindgen(js_name = "encodeRemoteAttachment")]
pub fn encode_remote_attachment(
  #[wasm_bindgen(js_name = "remoteAttachment")] remote_attachment: RemoteAttachment,
) -> Result<Uint8Array, WasmError> {
  // Use RemoteAttachmentCodec to encode the remote attachment
  let encoded = RemoteAttachmentCodec::encode(remote_attachment.into())
    .map_err(|e| WasmError::from_error(ErrorCode::ContentType, e))?;

  // Encode the EncodedContent to bytes
  let mut buf = Vec::new();
  encoded
    .encode(&mut buf)
    .map_err(|e| WasmError::from_error(ErrorCode::Encoding, e))?;

  Ok(Uint8Array::from(buf.as_slice()))
}

#[wasm_bindgen(js_name = "decodeRemoteAttachment")]
pub fn decode_remote_attachment(bytes: Uint8Array) -> Result<RemoteAttachment, WasmError> {
  // Decode bytes into EncodedContent
  let encoded_content = EncodedContent::decode(bytes.to_vec().as_slice())
    .map_err(|e| WasmError::from_error(ErrorCode::Encoding, e))?;

  // Use RemoteAttachmentCodec to decode into RemoteAttachment
  let attachment = RemoteAttachmentCodec::decode(encoded_content)
    .map_err(|e| WasmError::from_error(ErrorCode::ContentType, e))?;

  // Convert to bindings type with error handling
  RemoteAttachment::try_from(attachment)
}
