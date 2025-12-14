use crate::encoded_content::{ContentTypeId, EncodedContent};
use js_sys::Uint8Array;
use std::convert::TryFrom;
use wasm_bindgen::{JsError, prelude::wasm_bindgen};
use xmtp_content_types::ContentCodec;
use xmtp_content_types::remote_attachment::RemoteAttachmentCodec as XmtpRemoteAttachmentCodec;

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone)]
pub struct RemoteAttachment {
  pub url: String,
  #[wasm_bindgen(js_name = "contentDigest")]
  pub content_digest: String,
  pub secret: Uint8Array,
  pub salt: Uint8Array,
  pub nonce: Uint8Array,
  pub scheme: String,
  #[wasm_bindgen(js_name = "contentLength")]
  pub content_length: u32,
  pub filename: Option<String>,
}

#[wasm_bindgen]
impl RemoteAttachment {
  #[wasm_bindgen(constructor)]
  #[allow(clippy::too_many_arguments)]
  pub fn new(
    url: String,
    #[wasm_bindgen(js_name = "contentDigest")] content_digest: String,
    secret: Uint8Array,
    salt: Uint8Array,
    nonce: Uint8Array,
    scheme: String,
    #[wasm_bindgen(js_name = "contentLength")] content_length: u32,
    filename: Option<String>,
  ) -> Self {
    Self {
      url,
      content_digest,
      secret,
      salt,
      nonce,
      scheme,
      content_length,
      filename,
    }
  }
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
      secret: Uint8Array::from(remote.secret.as_slice()),
      salt: Uint8Array::from(remote.salt.as_slice()),
      nonce: Uint8Array::from(remote.nonce.as_slice()),
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
      secret: remote.secret.to_vec(),
      salt: remote.salt.to_vec(),
      nonce: remote.nonce.to_vec(),
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
