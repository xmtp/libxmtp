use crate::encoded_content::{ContentTypeId, EncodedContent};
use serde::{Deserialize, Serialize};
use tsify::Tsify;
use wasm_bindgen::JsError;
use wasm_bindgen::prelude::wasm_bindgen;
use xmtp_content_types::ContentCodec;
use xmtp_content_types::read_receipt::ReadReceiptCodec;

#[derive(Clone, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi, type = "Record<string, never>")]
pub struct ReadReceipt {}

impl From<xmtp_content_types::read_receipt::ReadReceipt> for ReadReceipt {
  fn from(_: xmtp_content_types::read_receipt::ReadReceipt) -> Self {
    Self {}
  }
}

impl From<ReadReceipt> for xmtp_content_types::read_receipt::ReadReceipt {
  fn from(_: ReadReceipt) -> Self {
    Self {}
  }
}

#[wasm_bindgen(js_name = "contentTypeReadReceipt")]
pub fn content_type_read_receipt() -> ContentTypeId {
  ReadReceiptCodec::content_type().into()
}

#[wasm_bindgen(js_name = "encodeReadReceipt")]
pub fn encode_read_receipt(read_receipt: ReadReceipt) -> Result<EncodedContent, JsError> {
  Ok(
    ReadReceiptCodec::encode(read_receipt.into())
      .map_err(|e| JsError::new(&format!("{}", e)))?
      .into(),
  )
}
