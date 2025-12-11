use crate::encoded_content::EncodedContent;
use js_sys::Uint8Array;
use prost::Message;
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
) -> Result<Uint8Array, JsError> {
  // Use ReadReceiptCodec to encode the read receipt
  let encoded =
    ReadReceiptCodec::encode(read_receipt.into()).map_err(|e| JsError::new(&format!("{}", e)))?;

  // Encode the EncodedContent to bytes
  let mut buf = Vec::new();
  encoded
    .encode(&mut buf)
    .map_err(|e| JsError::new(&format!("{}", e)))?;

  Ok(Uint8Array::from(buf.as_slice()))
}

#[wasm_bindgen(js_name = "decodeReadReceipt")]
pub fn decode_read_receipt(encoded_content: EncodedContent) -> Result<ReadReceipt, JsError> {
  // Use ReadReceiptCodec to decode into ReadReceipt and convert to ReadReceipt
  ReadReceiptCodec::decode(encoded_content.into())
    .map(Into::into)
    .map_err(|e| JsError::new(&format!("{}", e)))
}
