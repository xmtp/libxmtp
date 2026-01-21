use super::remote_attachment::RemoteAttachment;
use crate::ErrorWrapper;
use crate::messages::encoded_content::{ContentTypeId, EncodedContent};
use napi::bindgen_prelude::Result;
use napi_derive::napi;
use xmtp_content_types::ContentCodec;
use xmtp_content_types::multi_remote_attachment::MultiRemoteAttachmentCodec;
use xmtp_proto::xmtp::mls::message_contents::content_types::MultiRemoteAttachment as XmtpMultiRemoteAttachment;

#[napi(object)]
#[derive(Clone)]
pub struct MultiRemoteAttachment {
  pub attachments: Vec<RemoteAttachment>,
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
