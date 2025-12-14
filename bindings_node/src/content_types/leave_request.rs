use napi::bindgen_prelude::{Result, Uint8Array};
use napi_derive::napi;
use xmtp_content_types::{ContentCodec, leave_request::LeaveRequestCodec as XmtpLeaveRequestCodec};

use crate::{
  ErrorWrapper,
  encoded_content::{ContentTypeId, EncodedContent},
};

#[napi(object)]
pub struct LeaveRequest {
  pub authenticated_note: Option<Uint8Array>,
}

impl Clone for LeaveRequest {
  fn clone(&self) -> Self {
    Self {
      authenticated_note: self.authenticated_note.as_ref().map(|v| v.to_vec().into()),
    }
  }
}

impl From<xmtp_proto::xmtp::mls::message_contents::content_types::LeaveRequest> for LeaveRequest {
  fn from(lr: xmtp_proto::xmtp::mls::message_contents::content_types::LeaveRequest) -> Self {
    Self {
      authenticated_note: lr.authenticated_note.map(|v| v.into()),
    }
  }
}

#[napi]
pub struct LeaveRequestCodec {}

#[napi]
impl LeaveRequestCodec {
  #[napi]
  pub fn content_type() -> ContentTypeId {
    XmtpLeaveRequestCodec::content_type().into()
  }

  #[napi]
  pub fn decode(encoded_content: EncodedContent) -> Result<LeaveRequest> {
    Ok(
      XmtpLeaveRequestCodec::decode(encoded_content.into())
        .map(Into::into)
        .map_err(ErrorWrapper::from)?,
    )
  }

  #[napi]
  pub fn should_push() -> bool {
    XmtpLeaveRequestCodec::should_push()
  }
}
