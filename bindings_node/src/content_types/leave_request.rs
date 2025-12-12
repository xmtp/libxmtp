use napi::bindgen_prelude::{Result, Uint8Array};
use napi_derive::napi;
use prost::Message;
use xmtp_content_types::{ContentCodec, leave_request::LeaveRequestCodec};

use crate::{ErrorWrapper, encoded_content::EncodedContent};

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

impl From<LeaveRequest> for xmtp_proto::xmtp::mls::message_contents::content_types::LeaveRequest {
  fn from(lr: LeaveRequest) -> Self {
    Self {
      authenticated_note: lr.authenticated_note.map(|v| v.to_vec()),
    }
  }
}

#[napi]
pub fn encode_leave_request(leave_request: LeaveRequest) -> Result<Uint8Array> {
  let encoded = LeaveRequestCodec::encode(leave_request.into()).map_err(ErrorWrapper::from)?;

  let mut buf = Vec::new();
  encoded.encode(&mut buf).map_err(ErrorWrapper::from)?;

  Ok(buf.into())
}

#[napi]
pub fn decode_leave_request(encoded_content: EncodedContent) -> Result<LeaveRequest> {
  Ok(
    LeaveRequestCodec::decode(encoded_content.into())
      .map(Into::into)
      .map_err(ErrorWrapper::from)?,
  )
}
