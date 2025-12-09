use napi::bindgen_prelude::{Result, Uint8Array};
use napi_derive::napi;
use prost::Message;
use xmtp_content_types::{ContentCodec, leave_request::LeaveRequestCodec};
use xmtp_proto::xmtp::mls::message_contents::EncodedContent;

use crate::ErrorWrapper;

#[derive(Clone)]
#[napi(object)]
pub struct LeaveRequest {
  pub authenticated_note: Option<Vec<u8>>,
}

impl From<xmtp_proto::xmtp::mls::message_contents::content_types::LeaveRequest> for LeaveRequest {
  fn from(lr: xmtp_proto::xmtp::mls::message_contents::content_types::LeaveRequest) -> Self {
    Self {
      authenticated_note: lr.authenticated_note,
    }
  }
}

impl From<LeaveRequest> for xmtp_proto::xmtp::mls::message_contents::content_types::LeaveRequest {
  fn from(lr: LeaveRequest) -> Self {
    Self {
      authenticated_note: lr.authenticated_note,
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
pub fn decode_leave_request(bytes: Uint8Array) -> Result<LeaveRequest> {
  let encoded_content = EncodedContent::decode(bytes.as_ref()).map_err(ErrorWrapper::from)?;

  Ok(
    LeaveRequestCodec::decode(encoded_content)
      .map(Into::into)
      .map_err(ErrorWrapper::from)?,
  )
}
