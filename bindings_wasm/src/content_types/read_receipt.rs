use crate::encoded_content::EncodedContent;
use wasm_bindgen::{JsError, prelude::wasm_bindgen};
use xmtp_content_types::ContentCodec;
use xmtp_content_types::read_receipt::ReadReceiptCodec;

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone)]
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

#[wasm_bindgen(js_name = "encodeReadReceipt")]
pub fn encode_read_receipt(
  #[wasm_bindgen(js_name = "readReceipt")] read_receipt: ReadReceipt,
) -> Result<EncodedContent, JsError> {
  let encoded_content =
    ReadReceiptCodec::encode(read_receipt.into()).map_err(|e| JsError::new(&format!("{}", e)))?;
  Ok(encoded_content.into())
}

#[wasm_bindgen(js_name = "decodeReadReceipt")]
pub fn decode_read_receipt(encoded_content: EncodedContent) -> Result<ReadReceipt, JsError> {
  ReadReceiptCodec::decode(encoded_content.into())
    .map(Into::into)
    .map_err(|e| JsError::new(&format!("{}", e)))
}
