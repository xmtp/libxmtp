use crate::encoded_content::ContentTypeId;
use serde::{Deserialize, Serialize};
use tsify::Tsify;
use wasm_bindgen::prelude::wasm_bindgen;
use xmtp_content_types::ContentCodec;
use xmtp_content_types::leave_request::LeaveRequestCodec;

#[derive(Clone, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct LeaveRequest {
  #[serde(skip_serializing_if = "Option::is_none", with = "serde_bytes", default)]
  #[tsify(optional, type = "Uint8Array")]
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

#[wasm_bindgen(js_name = "contentTypeLeaveRequest")]
pub fn content_type_leave_request() -> ContentTypeId {
  LeaveRequestCodec::content_type().into()
}
