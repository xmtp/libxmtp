use napi::bindgen_prelude::{Result, Uint8Array};
use napi_derive::napi;
use xmtp_content_types::ContentCodec;
use xmtp_content_types::multi_remote_attachment::MultiRemoteAttachmentCodec;
use xmtp_proto::xmtp::mls::message_contents::content_types::{
  MultiRemoteAttachment as XmtpMultiRemoteAttachment,
  RemoteAttachmentInfo as XmtpRemoteAttachmentInfo,
};

use crate::ErrorWrapper;
use crate::encoded_content::{ContentTypeId, EncodedContent};

#[napi(object)]
pub struct RemoteAttachmentInfo {
  pub secret: Uint8Array,
  pub content_digest: String,
  pub nonce: Uint8Array,
  pub scheme: String,
  pub url: String,
  pub salt: Uint8Array,
  pub content_length: Option<u32>,
  pub filename: Option<String>,
}

impl Clone for RemoteAttachmentInfo {
  fn clone(&self) -> Self {
    Self {
      secret: Uint8Array::from(self.secret.to_vec()),
      content_digest: self.content_digest.clone(),
      nonce: Uint8Array::from(self.nonce.to_vec()),
      scheme: self.scheme.clone(),
      url: self.url.clone(),
      salt: Uint8Array::from(self.salt.to_vec()),
      content_length: self.content_length,
      filename: self.filename.clone(),
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
      secret: remote_attachment_info.secret.into(),
      content_digest: remote_attachment_info.content_digest,
      nonce: remote_attachment_info.nonce.into(),
      scheme: remote_attachment_info.scheme,
      url: remote_attachment_info.url,
      salt: remote_attachment_info.salt.into(),
      content_length: remote_attachment_info.content_length,
      filename: remote_attachment_info.filename,
    }
  }
}

#[napi(object)]
#[derive(Clone)]
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

#[napi]
pub fn content_type_multi_remote_attachment() -> ContentTypeId {
  MultiRemoteAttachmentCodec::content_type().into()
}

#[napi]
pub fn encode_multi_remote_attachment(
  multi_remote_attachment: MultiRemoteAttachment,
) -> Result<EncodedContent> {
  Ok(
    MultiRemoteAttachmentCodec::encode(multi_remote_attachment.into())
      .map_err(ErrorWrapper::from)?
      .into(),
  )
}
