use napi::bindgen_prelude::Result;
use napi_derive::napi;
use xmtp_content_types::{
  ContentCodec, transaction_reference::TransactionReferenceCodec as XmtpTransactionReferenceCodec,
};

use crate::{
  ErrorWrapper,
  encoded_content::{ContentTypeId, EncodedContent},
};

#[derive(Clone)]
#[napi(object)]
pub struct TransactionReference {
  pub namespace: Option<String>,
  pub network_id: String,
  pub reference: String,
  pub metadata: Option<TransactionMetadata>,
}

impl From<xmtp_content_types::transaction_reference::TransactionReference>
  for TransactionReference
{
  fn from(tr: xmtp_content_types::transaction_reference::TransactionReference) -> Self {
    Self {
      namespace: tr.namespace,
      network_id: tr.network_id,
      reference: tr.reference,
      metadata: tr.metadata.map(Into::into),
    }
  }
}

impl From<TransactionReference>
  for xmtp_content_types::transaction_reference::TransactionReference
{
  fn from(tr: TransactionReference) -> Self {
    Self {
      namespace: tr.namespace,
      network_id: tr.network_id,
      reference: tr.reference,
      metadata: tr.metadata.map(Into::into),
    }
  }
}

#[derive(Clone)]
#[napi(object)]
pub struct TransactionMetadata {
  pub transaction_type: String,
  pub currency: String,
  pub amount: f64,
  pub decimals: u32,
  pub from_address: String,
  pub to_address: String,
}

impl From<xmtp_content_types::transaction_reference::TransactionMetadata> for TransactionMetadata {
  fn from(meta: xmtp_content_types::transaction_reference::TransactionMetadata) -> Self {
    Self {
      transaction_type: meta.transaction_type,
      currency: meta.currency,
      amount: meta.amount,
      decimals: meta.decimals,
      from_address: meta.from_address,
      to_address: meta.to_address,
    }
  }
}

impl From<TransactionMetadata> for xmtp_content_types::transaction_reference::TransactionMetadata {
  fn from(meta: TransactionMetadata) -> Self {
    Self {
      transaction_type: meta.transaction_type,
      currency: meta.currency,
      amount: meta.amount,
      decimals: meta.decimals,
      from_address: meta.from_address,
      to_address: meta.to_address,
    }
  }
}

#[napi]
pub struct TransactionReferenceCodec {}

#[napi]
impl TransactionReferenceCodec {
  #[napi]
  pub fn content_type() -> ContentTypeId {
    XmtpTransactionReferenceCodec::content_type().into()
  }

  #[napi]
  pub fn encode(transaction_reference: TransactionReference) -> Result<EncodedContent> {
    let encoded_content = XmtpTransactionReferenceCodec::encode(transaction_reference.into())
      .map_err(ErrorWrapper::from)?;
    Ok(encoded_content.into())
  }

  #[napi]
  pub fn decode(encoded_content: EncodedContent) -> Result<TransactionReference> {
    Ok(
      XmtpTransactionReferenceCodec::decode(encoded_content.into())
        .map(Into::into)
        .map_err(ErrorWrapper::from)?,
    )
  }

  #[napi]
  pub fn should_push() -> bool {
    XmtpTransactionReferenceCodec::should_push()
  }
}
