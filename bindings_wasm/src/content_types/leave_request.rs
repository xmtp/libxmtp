use crate::encoded_content::EncodedContent;
use js_sys::Uint8Array;
use prost::Message;
use wasm_bindgen::{JsError, prelude::wasm_bindgen};
use xmtp_content_types::{ContentCodec, leave_request::LeaveRequestCodec};

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

#[wasm_bindgen(js_name = "encodeLeaveRequest")]
pub fn encode_leave_request(
  #[wasm_bindgen(js_name = "leaveRequest")] leave_request: LeaveRequest,
) -> Result<Uint8Array, JsError> {
  let encoded =
    LeaveRequestCodec::encode(leave_request.into()).map_err(|e| JsError::new(&format!("{}", e)))?;

  let mut buf = Vec::new();
  encoded
    .encode(&mut buf)
    .map_err(|e| JsError::new(&format!("{}", e)))?;

  Ok(Uint8Array::from(buf.as_slice()))
}

#[wasm_bindgen(js_name = "decodeLeaveRequest")]
pub fn decode_leave_request(encoded_content: EncodedContent) -> Result<LeaveRequest, JsError> {
  LeaveRequestCodec::decode(encoded_content.into())
    .map(Into::into)
    .map_err(|e| JsError::new(&format!("{}", e)))
}
