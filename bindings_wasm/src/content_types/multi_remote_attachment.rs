use crate::encoded_content::{ContentTypeId, EncodedContent};
use js_sys::Uint8Array;
use wasm_bindgen::{JsError, prelude::wasm_bindgen};
use xmtp_content_types::ContentCodec;
use xmtp_content_types::multi_remote_attachment::MultiRemoteAttachmentCodec as XmtpMultiRemoteAttachmentCodec;
use xmtp_proto::xmtp::mls::message_contents::content_types::{
  MultiRemoteAttachment as XmtpMultiRemoteAttachment,
  RemoteAttachmentInfo as XmtpRemoteAttachmentInfo,
};

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone, Default)]
pub struct RemoteAttachmentInfo {
  pub secret: Uint8Array,
  #[wasm_bindgen(js_name = "contentDigest")]
  pub content_digest: String,
  pub nonce: Uint8Array,
  pub scheme: String,
  pub url: String,
  pub salt: Uint8Array,
  #[wasm_bindgen(js_name = "contentLength")]
  pub content_length: Option<u32>,
  pub filename: Option<String>,
}

#[wasm_bindgen]
impl RemoteAttachmentInfo {
  #[wasm_bindgen(constructor)]
  #[allow(clippy::too_many_arguments)]
  pub fn new(
    secret: Uint8Array,
    #[wasm_bindgen(js_name = "contentDigest")] content_digest: String,
    nonce: Uint8Array,
    scheme: String,
    url: String,
    salt: Uint8Array,
    #[wasm_bindgen(js_name = "contentLength")] content_length: Option<u32>,
    filename: Option<String>,
  ) -> Self {
    Self {
      secret,
      content_digest,
      nonce,
      scheme,
      url,
      salt,
      content_length,
      filename,
    }
  }
}

impl From<RemoteAttachmentInfo> for XmtpRemoteAttachmentInfo {
  fn from(remote_attachment_info: RemoteAttachmentInfo) -> Self {
    XmtpRemoteAttachmentInfo {
      content_digest: remote_attachment_info.content_digest,
      secret: remote_attachment_info.secret.to_vec(),
      nonce: remote_attachment_info.nonce.to_vec(),
      salt: remote_attachment_info.salt.to_vec(),
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
      secret: Uint8Array::from(remote_attachment_info.secret.as_slice()),
      content_digest: remote_attachment_info.content_digest,
      nonce: Uint8Array::from(remote_attachment_info.nonce.as_slice()),
      scheme: remote_attachment_info.scheme,
      url: remote_attachment_info.url,
      salt: Uint8Array::from(remote_attachment_info.salt.as_slice()),
      content_length: remote_attachment_info.content_length,
      filename: remote_attachment_info.filename,
    }
  }
}

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone, Default)]
pub struct MultiRemoteAttachment {
  pub attachments: Vec<RemoteAttachmentInfo>,
}

#[wasm_bindgen]
impl MultiRemoteAttachment {
  #[wasm_bindgen(constructor)]
  pub fn new(attachments: Vec<RemoteAttachmentInfo>) -> Self {
    Self { attachments }
  }
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
