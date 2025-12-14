use crate::encoded_content::{ContentTypeId, EncodedContent};
use js_sys::Uint8Array;
use wasm_bindgen::{JsError, prelude::wasm_bindgen};
use xmtp_content_types::{ContentCodec, leave_request::LeaveRequestCodec as XmtpLeaveRequestCodec};

#[wasm_bindgen]
pub struct LeaveRequest {
  #[wasm_bindgen(getter_with_clone, js_name = "authenticatedNote")]
  pub authenticated_note: Option<Uint8Array>,
}

impl Clone for LeaveRequest {
  fn clone(&self) -> Self {
    Self {
      authenticated_note: self
        .authenticated_note
        .as_ref()
        .map(|v| Uint8Array::from(v.to_vec().as_slice())),
    }
  }
}

impl From<xmtp_proto::xmtp::mls::message_contents::content_types::LeaveRequest> for LeaveRequest {
  fn from(lr: xmtp_proto::xmtp::mls::message_contents::content_types::LeaveRequest) -> Self {
    Self {
      authenticated_note: lr
        .authenticated_note
        .map(|v| Uint8Array::from(v.as_slice())),
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

#[wasm_bindgen]
pub struct LeaveRequestCodec;

#[wasm_bindgen]
impl LeaveRequestCodec {
  #[wasm_bindgen(js_name = "contentType")]
  pub fn content_type() -> ContentTypeId {
    XmtpLeaveRequestCodec::content_type().into()
  }

  #[wasm_bindgen]
  pub fn decode(encoded_content: EncodedContent) -> Result<LeaveRequest, JsError> {
    XmtpLeaveRequestCodec::decode(encoded_content.into())
      .map(Into::into)
      .map_err(|e| JsError::new(&format!("{}", e)))
  }

  #[wasm_bindgen(js_name = "shouldPush")]
  pub fn should_push() -> bool {
    XmtpLeaveRequestCodec::should_push()
  }
}
