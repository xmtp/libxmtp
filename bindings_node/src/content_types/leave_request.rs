use napi::bindgen_prelude::Uint8Array;
use napi_derive::napi;
use xmtp_content_types::{ContentCodec, leave_request::LeaveRequestCodec};

use crate::encoded_content::ContentTypeId;

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
pub fn content_type_leave_request() -> ContentTypeId {
  LeaveRequestCodec::content_type().into()
}
