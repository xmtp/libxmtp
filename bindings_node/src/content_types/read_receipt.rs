use napi::bindgen_prelude::{Result, Uint8Array};
use napi_derive::napi;
use prost::Message;
use xmtp_content_types::{ContentCodec, read_receipt::ReadReceiptCodec};

use crate::{ErrorWrapper, encoded_content::EncodedContent};

#[derive(Clone)]
#[napi(object)]
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

#[napi]
pub fn encode_read_receipt(read_receipt: ReadReceipt) -> Result<Uint8Array> {
  // Use ReadReceiptCodec to encode the read receipt
  let encoded = ReadReceiptCodec::encode(read_receipt.into()).map_err(ErrorWrapper::from)?;

  // Encode the EncodedContent to bytes
  let mut buf = Vec::new();
  encoded.encode(&mut buf).map_err(ErrorWrapper::from)?;

  Ok(buf.into())
}

#[napi]
pub fn decode_read_receipt(encoded_content: EncodedContent) -> Result<ReadReceipt> {
  Ok(
    ReadReceiptCodec::decode(encoded_content.into())
      .map(Into::into)
      .map_err(ErrorWrapper::from)?,
  )
}
