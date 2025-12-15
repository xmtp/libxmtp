use crate::encoded_content::{ContentTypeId, EncodedContent};
use serde::{Deserialize, Serialize};
use tsify::Tsify;
use wasm_bindgen::JsError;
use wasm_bindgen::prelude::wasm_bindgen;
use xmtp_content_types::{ContentCodec, leave_request::LeaveRequestCodec as XmtpLeaveRequestCodec};

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
