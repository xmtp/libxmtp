use crate::ErrorWrapper;
use crate::messages::encoded_content::{ContentTypeId, EncodedContent};
use napi::bindgen_prelude::Result;
use napi_derive::napi;
use xmtp_content_types::{ContentCodec, read_receipt::ReadReceiptCodec};

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
pub fn content_type_read_receipt() -> ContentTypeId {
  ReadReceiptCodec::content_type().into()
}

#[napi]
pub fn encode_read_receipt(read_receipt: ReadReceipt) -> Result<EncodedContent> {
  Ok(
    ReadReceiptCodec::encode(read_receipt.into())
      .map_err(ErrorWrapper::from)?
      .into(),
  )
}
