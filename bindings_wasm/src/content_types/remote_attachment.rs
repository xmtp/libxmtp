use js_sys::Uint8Array;
use prost::Message;
use wasm_bindgen::{JsError, prelude::wasm_bindgen};
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

impl From<xmtp_content_types::remote_attachment::RemoteAttachment> for RemoteAttachment {
  fn from(remote: xmtp_content_types::remote_attachment::RemoteAttachment) -> Self {
    Self {
      url: remote.url,
      content_digest: remote.content_digest,
      secret: remote.secret,
      salt: remote.salt,
      nonce: remote.nonce,
      scheme: remote.scheme,
      content_length: remote.content_length as u32,
      filename: remote.filename,
    }
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
) -> Result<Uint8Array, JsError> {
  // Use RemoteAttachmentCodec to encode the remote attachment
  let encoded = RemoteAttachmentCodec::encode(remote_attachment.into())
    .map_err(|e| JsError::new(&format!("{}", e)))?;

  // Encode the EncodedContent to bytes
  let mut buf = Vec::new();
  encoded
    .encode(&mut buf)
    .map_err(|e| JsError::new(&format!("{}", e)))?;

  Ok(Uint8Array::from(buf.as_slice()))
}

#[wasm_bindgen(js_name = "decodeRemoteAttachment")]
pub fn decode_remote_attachment(bytes: Uint8Array) -> Result<RemoteAttachment, JsError> {
  // Decode bytes into EncodedContent
  let encoded_content = EncodedContent::decode(bytes.to_vec().as_slice())
    .map_err(|e| JsError::new(&format!("{}", e)))?;

  // Use RemoteAttachmentCodec to decode into RemoteAttachment and convert to RemoteAttachment
  RemoteAttachmentCodec::decode(encoded_content)
    .map(Into::into)
    .map_err(|e| JsError::new(&format!("{}", e)))
}
