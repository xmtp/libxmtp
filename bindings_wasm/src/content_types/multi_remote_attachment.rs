use crate::encoded_content::{ContentTypeId, EncodedContent};
use serde::{Deserialize, Serialize};
use tsify::Tsify;
use wasm_bindgen::JsError;
use wasm_bindgen::prelude::wasm_bindgen;
use xmtp_content_types::ContentCodec;
use xmtp_content_types::multi_remote_attachment::MultiRemoteAttachmentCodec as XmtpMultiRemoteAttachmentCodec;
use xmtp_proto::xmtp::mls::message_contents::content_types::{
  MultiRemoteAttachment as XmtpMultiRemoteAttachment,
  RemoteAttachmentInfo as XmtpRemoteAttachmentInfo,
};

#[derive(Clone, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct RemoteAttachmentInfo {
  #[serde(with = "serde_bytes")]
  #[tsify(type = "Uint8Array")]
  pub secret: Vec<u8>,
  pub content_digest: String,
  #[serde(with = "serde_bytes")]
  #[tsify(type = "Uint8Array")]
  pub nonce: Vec<u8>,
  pub scheme: String,
  pub url: String,
  #[serde(with = "serde_bytes")]
  #[tsify(type = "Uint8Array")]
  pub salt: Vec<u8>,
  #[serde(skip_serializing_if = "Option::is_none")]
  #[tsify(optional)]
  pub content_length: Option<u32>,
  #[serde(skip_serializing_if = "Option::is_none")]
  #[tsify(optional)]
  pub filename: Option<String>,
}

impl From<RemoteAttachmentInfo> for XmtpRemoteAttachmentInfo {
  fn from(info: RemoteAttachmentInfo) -> Self {
    XmtpRemoteAttachmentInfo {
      content_digest: info.content_digest,
      secret: info.secret,
      nonce: info.nonce,
      salt: info.salt,
      scheme: info.scheme,
      url: info.url,
      content_length: info.content_length,
      filename: info.filename,
    }
  }
}

impl From<XmtpRemoteAttachmentInfo> for RemoteAttachmentInfo {
  fn from(info: XmtpRemoteAttachmentInfo) -> Self {
    RemoteAttachmentInfo {
      secret: info.secret,
      content_digest: info.content_digest,
      nonce: info.nonce,
      scheme: info.scheme,
      url: info.url,
      salt: info.salt,
      content_length: info.content_length,
      filename: info.filename,
    }
  }
}

#[derive(Clone, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct MultiRemoteAttachment {
  pub attachments: Vec<RemoteAttachmentInfo>,
}

impl From<MultiRemoteAttachment> for XmtpMultiRemoteAttachment {
  fn from(multi: MultiRemoteAttachment) -> Self {
    XmtpMultiRemoteAttachment {
      attachments: multi.attachments.into_iter().map(Into::into).collect(),
    }
  }
}

impl From<XmtpMultiRemoteAttachment> for MultiRemoteAttachment {
  fn from(multi: XmtpMultiRemoteAttachment) -> Self {
    MultiRemoteAttachment {
      attachments: multi.attachments.into_iter().map(Into::into).collect(),
    }
  }
}

#[wasm_bindgen]
pub struct MultiRemoteAttachmentCodec;

#[wasm_bindgen]
impl MultiRemoteAttachmentCodec {
  #[wasm_bindgen(js_name = "contentType")]
  pub fn content_type() -> ContentTypeId {
    XmtpMultiRemoteAttachmentCodec::content_type().into()
  }

  #[wasm_bindgen]
  pub fn encode(multi_remote_attachment: MultiRemoteAttachment) -> Result<EncodedContent, JsError> {
    let multi_remote_attachment: XmtpMultiRemoteAttachment = multi_remote_attachment.into();
    let encoded_content = XmtpMultiRemoteAttachmentCodec::encode(multi_remote_attachment)
      .map_err(|e| JsError::new(&format!("{}", e)))?;
    Ok(encoded_content.into())
  }

  #[wasm_bindgen]
  pub fn decode(encoded_content: EncodedContent) -> Result<MultiRemoteAttachment, JsError> {
    XmtpMultiRemoteAttachmentCodec::decode(encoded_content.into())
      .map(Into::into)
      .map_err(|e| JsError::new(&format!("{}", e)))
  }

  #[wasm_bindgen(js_name = "shouldPush")]
  pub fn should_push() -> bool {
    XmtpMultiRemoteAttachmentCodec::should_push()
  }
}
