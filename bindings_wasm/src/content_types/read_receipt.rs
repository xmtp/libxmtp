use crate::encoded_content::{ContentTypeId, EncodedContent};
use serde::{Deserialize, Serialize};
use tsify::Tsify;
use wasm_bindgen::JsError;
use wasm_bindgen::prelude::wasm_bindgen;
use xmtp_content_types::ContentCodec;
use xmtp_content_types::read_receipt::ReadReceiptCodec as XmtpReadReceiptCodec;

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

#[wasm_bindgen]
pub struct ReadReceiptCodec;

#[wasm_bindgen]
impl ReadReceiptCodec {
  #[wasm_bindgen(js_name = "contentType")]
  pub fn content_type() -> ContentTypeId {
    XmtpReadReceiptCodec::content_type().into()
  }

  #[wasm_bindgen]
  pub fn encode() -> Result<EncodedContent, JsError> {
    let encoded_content = XmtpReadReceiptCodec::encode(ReadReceipt {}.into())
      .map_err(|e| JsError::new(&format!("{}", e)))?;
    Ok(encoded_content.into())
  }

  #[wasm_bindgen]
  pub fn decode(encoded_content: EncodedContent) -> Result<ReadReceipt, JsError> {
    XmtpReadReceiptCodec::decode(encoded_content.into())
      .map(Into::into)
      .map_err(|e| JsError::new(&format!("{}", e)))
  }

  #[wasm_bindgen(js_name = "shouldPush")]
  pub fn should_push() -> bool {
    XmtpReadReceiptCodec::should_push()
  }
}
