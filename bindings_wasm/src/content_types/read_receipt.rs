use crate::error::{ErrorCode, WasmError};
use js_sys::Uint8Array;
use prost::Message;
use wasm_bindgen::prelude::wasm_bindgen;
use xmtp_content_types::ContentCodec;
use xmtp_content_types::read_receipt::ReadReceiptCodec;
use xmtp_proto::xmtp::mls::message_contents::EncodedContent;

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
) -> Result<Uint8Array, WasmError> {
  // Use ReadReceiptCodec to encode the read receipt
  let encoded = ReadReceiptCodec::encode(read_receipt.into())
    .map_err(|e| WasmError::from_error(ErrorCode::ContentType, e))?;

  // Encode the EncodedContent to bytes
  let mut buf = Vec::new();
  encoded
    .encode(&mut buf)
    .map_err(|e| WasmError::from_error(ErrorCode::Encoding, e))?;

  Ok(Uint8Array::from(buf.as_slice()))
}

#[wasm_bindgen(js_name = "decodeReadReceipt")]
pub fn decode_read_receipt(bytes: Uint8Array) -> Result<ReadReceipt, WasmError> {
  // Decode bytes into EncodedContent
  let encoded_content = EncodedContent::decode(bytes.to_vec().as_slice())
    .map_err(|e| WasmError::from_error(ErrorCode::Encoding, e))?;

  // Use ReadReceiptCodec to decode into ReadReceipt and convert to ReadReceipt
  ReadReceiptCodec::decode(encoded_content)
    .map(Into::into)
    .map_err(|e| WasmError::from_error(ErrorCode::ContentType, e))
}
