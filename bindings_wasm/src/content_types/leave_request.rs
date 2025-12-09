use js_sys::Uint8Array;
use prost::Message;
use wasm_bindgen::{JsError, prelude::wasm_bindgen};
use xmtp_content_types::{ContentCodec, leave_request::LeaveRequestCodec};
use xmtp_proto::xmtp::mls::message_contents::EncodedContent;

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone)]
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
pub fn decode_leave_request(bytes: Uint8Array) -> Result<LeaveRequest, JsError> {
  let encoded_content = EncodedContent::decode(bytes.to_vec().as_slice())
    .map_err(|e| JsError::new(&format!("{}", e)))?;

  LeaveRequestCodec::decode(encoded_content)
    .map(Into::into)
    .map_err(|e| JsError::new(&format!("{}", e)))
}
