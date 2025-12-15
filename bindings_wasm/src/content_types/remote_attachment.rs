use crate::encoded_content::{ContentTypeId, EncodedContent};
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;
use tsify::Tsify;
use wasm_bindgen::JsError;
use wasm_bindgen::prelude::wasm_bindgen;
use xmtp_content_types::ContentCodec;
use xmtp_content_types::remote_attachment::RemoteAttachmentCodec as XmtpRemoteAttachmentCodec;

#[derive(Clone, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct RemoteAttachment {
  pub url: String,
  pub content_digest: String,
  #[serde(with = "serde_bytes")]
  #[tsify(type = "Uint8Array")]
  pub secret: Vec<u8>,
  #[serde(with = "serde_bytes")]
  #[tsify(type = "Uint8Array")]
  pub salt: Vec<u8>,
  #[serde(with = "serde_bytes")]
  #[tsify(type = "Uint8Array")]
  pub nonce: Vec<u8>,
  pub scheme: String,
  pub content_length: u32,
  #[serde(skip_serializing_if = "Option::is_none")]
  #[tsify(optional)]
  pub filename: Option<String>,
}

impl TryFrom<xmtp_content_types::remote_attachment::RemoteAttachment> for RemoteAttachment {
  type Error = JsError;

  fn try_from(
    remote: xmtp_content_types::remote_attachment::RemoteAttachment,
  ) -> std::result::Result<Self, Self::Error> {
    let content_length = u32::try_from(remote.content_length).map_err(|_| {
      JsError::new(&format!(
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

#[wasm_bindgen]
pub struct RemoteAttachmentCodec;

#[wasm_bindgen]
impl RemoteAttachmentCodec {
  #[wasm_bindgen(js_name = "contentType")]
  pub fn content_type() -> ContentTypeId {
    XmtpRemoteAttachmentCodec::content_type().into()
  }

  #[wasm_bindgen]
  pub fn encode(remote_attachment: RemoteAttachment) -> Result<EncodedContent, JsError> {
    let encoded_content = XmtpRemoteAttachmentCodec::encode(remote_attachment.into())
      .map_err(|e| JsError::new(&format!("{}", e)))?;
    Ok(encoded_content.into())
  }

  #[wasm_bindgen]
  pub fn decode(encoded_content: EncodedContent) -> Result<RemoteAttachment, JsError> {
    let attachment = XmtpRemoteAttachmentCodec::decode(encoded_content.into())
      .map_err(|e| JsError::new(&format!("{}", e)))?;
    RemoteAttachment::try_from(attachment)
  }

  #[wasm_bindgen(js_name = "shouldPush")]
  pub fn should_push() -> bool {
    XmtpRemoteAttachmentCodec::should_push()
  }
}
