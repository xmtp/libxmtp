use js_sys::Uint8Array;
use prost::Message;
use serde::{Deserialize, Serialize};
use tsify_next::Tsify;
use wasm_bindgen::{prelude::wasm_bindgen, JsError};
use xmtp_content_types::multi_remote_attachment::MultiRemoteAttachmentCodec;
use xmtp_content_types::ContentCodec;
use xmtp_proto::xmtp::mls::message_contents::content_types::{
  MultiRemoteAttachment as XmtpMultiRemoteAttachment,
  RemoteAttachmentInfo as XmtpRemoteAttachmentInfo,
};
use xmtp_proto::xmtp::mls::message_contents::EncodedContent;

#[derive(Tsify, Clone, Serialize, Deserialize)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct RemoteAttachmentInfo {
  pub secret: Vec<u8>,
  #[serde(rename = "contentDigest")]
  pub content_digest: String,
  pub nonce: Vec<u8>,
  pub scheme: String,
  pub url: String,
  pub salt: Vec<u8>,
  #[serde(rename = "contentLength")]
  pub content_length: Option<u32>,
  pub filename: Option<String>,
}

impl From<RemoteAttachmentInfo> for XmtpRemoteAttachmentInfo {
  fn from(remote_attachment_info: RemoteAttachmentInfo) -> Self {
    XmtpRemoteAttachmentInfo {
      content_digest: remote_attachment_info.content_digest,
      secret: remote_attachment_info.secret,
      nonce: remote_attachment_info.nonce,
      salt: remote_attachment_info.salt,
      scheme: remote_attachment_info.scheme,
      url: remote_attachment_info.url,
      content_length: remote_attachment_info.content_length,
      filename: remote_attachment_info.filename,
    }
  }
}

impl From<XmtpRemoteAttachmentInfo> for RemoteAttachmentInfo {
  fn from(remote_attachment_info: XmtpRemoteAttachmentInfo) -> Self {
    RemoteAttachmentInfo {
      secret: remote_attachment_info.secret,
      content_digest: remote_attachment_info.content_digest,
      nonce: remote_attachment_info.nonce,
      scheme: remote_attachment_info.scheme,
      url: remote_attachment_info.url,
      salt: remote_attachment_info.salt,
      content_length: remote_attachment_info.content_length,
      filename: remote_attachment_info.filename,
    }
  }
}

#[derive(Tsify, Clone, Default, Serialize, Deserialize)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct MultiRemoteAttachment {
  pub attachments: Vec<RemoteAttachmentInfo>,
}

impl From<MultiRemoteAttachment> for XmtpMultiRemoteAttachment {
  fn from(multi_remote_attachment: MultiRemoteAttachment) -> Self {
    XmtpMultiRemoteAttachment {
      attachments: multi_remote_attachment
        .attachments
        .into_iter()
        .map(Into::into)
        .collect(),
    }
  }
}

impl From<XmtpMultiRemoteAttachment> for MultiRemoteAttachment {
  fn from(multi_remote_attachment: XmtpMultiRemoteAttachment) -> Self {
    MultiRemoteAttachment {
      attachments: multi_remote_attachment
        .attachments
        .into_iter()
        .map(Into::into)
        .collect(),
    }
  }
}

#[wasm_bindgen(js_name = "encodeMultiRemoteAttachment")]
pub fn encode_multi_remote_attachment(
  #[wasm_bindgen(js_name = "multiRemoteAttachment")] multi_remote_attachment: MultiRemoteAttachment,
) -> Result<Uint8Array, JsError> {
  // Convert MultiRemoteAttachment to MultiRemoteAttachment
  let multi_remote_attachment: XmtpMultiRemoteAttachment = multi_remote_attachment.into();

  // Use MultiRemoteAttachmentCodec to encode the attachments
  let encoded = MultiRemoteAttachmentCodec::encode(multi_remote_attachment)
    .map_err(|e| JsError::new(&format!("{}", e)))?;

  // Encode the EncodedContent to bytes
  let mut buf = Vec::new();
  encoded
    .encode(&mut buf)
    .map_err(|e| JsError::new(&format!("{}", e)))?;

  Ok(Uint8Array::from(buf.as_slice()))
}

#[wasm_bindgen(js_name = "decodeMultiRemoteAttachment")]
pub fn decode_multi_remote_attachment(bytes: Uint8Array) -> Result<MultiRemoteAttachment, JsError> {
  // Decode bytes into EncodedContent
  let encoded_content = EncodedContent::decode(bytes.to_vec().as_slice())
    .map_err(|e| JsError::new(&format!("{}", e)))?;

  // Use MultiRemoteAttachmentCodec to decode into MultiRemoteAttachment and convert to MultiRemoteAttachment
  MultiRemoteAttachmentCodec::decode(encoded_content)
    .map(Into::into)
    .map_err(|e| JsError::new(&format!("{}", e)))
}
