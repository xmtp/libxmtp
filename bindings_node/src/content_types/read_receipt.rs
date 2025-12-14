use napi::bindgen_prelude::Result;
use napi_derive::napi;
use xmtp_content_types::{ContentCodec, read_receipt::ReadReceiptCodec as XmtpReadReceiptCodec};

use crate::{
  ErrorWrapper,
  encoded_content::{ContentTypeId, EncodedContent},
};

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
pub struct ReadReceiptCodec {}

#[napi]
impl ReadReceiptCodec {
  #[napi]
  pub fn content_type() -> ContentTypeId {
    XmtpReadReceiptCodec::content_type().into()
  }

  #[napi]
  pub fn encode(read_receipt: ReadReceipt) -> Result<EncodedContent> {
    let encoded_content =
      XmtpReadReceiptCodec::encode(read_receipt.into()).map_err(ErrorWrapper::from)?;
    Ok(encoded_content.into())
  }

  #[napi]
  pub fn decode(encoded_content: EncodedContent) -> Result<ReadReceipt> {
    Ok(
      XmtpReadReceiptCodec::decode(encoded_content.into())
        .map(Into::into)
        .map_err(ErrorWrapper::from)?,
    )
  }

  #[napi]
  pub fn should_push() -> bool {
    XmtpReadReceiptCodec::should_push()
  }
}
