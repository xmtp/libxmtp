use crate::encoded_content::{ContentTypeId, EncodedContent};
use wasm_bindgen::{JsError, prelude::wasm_bindgen};
use xmtp_content_types::ContentCodec;
use xmtp_content_types::read_receipt::ReadReceiptCodec as XmtpReadReceiptCodec;

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone)]
pub struct ReadReceipt {}

#[wasm_bindgen]
impl ReadReceipt {
  #[wasm_bindgen(constructor)]
  pub fn new() -> Self {
    Self {}
  }
}

impl Default for ReadReceipt {
  fn default() -> Self {
    Self::new()
  }
}

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
  pub fn encode(read_receipt: ReadReceipt) -> Result<EncodedContent, JsError> {
    let encoded_content = XmtpReadReceiptCodec::encode(read_receipt.into())
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
