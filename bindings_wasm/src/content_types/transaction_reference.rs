use crate::encoded_content::{ContentTypeId, EncodedContent};
use wasm_bindgen::{JsError, prelude::wasm_bindgen};
use xmtp_content_types::ContentCodec;
use xmtp_content_types::transaction_reference::TransactionReferenceCodec as XmtpTransactionReferenceCodec;

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone)]
pub struct TransactionReference {
  pub namespace: Option<String>,
  #[wasm_bindgen(js_name = "networkId")]
  pub network_id: String,
  pub reference: String,
  pub metadata: Option<TransactionMetadata>,
}

#[wasm_bindgen]
impl TransactionReference {
  #[wasm_bindgen(constructor)]
  pub fn new(
    namespace: Option<String>,
    #[wasm_bindgen(js_name = "networkId")] network_id: String,
    reference: String,
    metadata: Option<TransactionMetadata>,
  ) -> Self {
    Self {
      namespace,
      network_id,
      reference,
      metadata,
    }
  }
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

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone)]
pub struct TransactionMetadata {
  #[wasm_bindgen(js_name = "transactionType")]
  pub transaction_type: String,
  pub currency: String,
  pub amount: f64,
  pub decimals: u32,
  #[wasm_bindgen(js_name = "fromAddress")]
  pub from_address: String,
  #[wasm_bindgen(js_name = "toAddress")]
  pub to_address: String,
}

#[wasm_bindgen]
impl TransactionMetadata {
  #[wasm_bindgen(constructor)]
  pub fn new(
    #[wasm_bindgen(js_name = "transactionType")] transaction_type: String,
    currency: String,
    amount: f64,
    decimals: u32,
    #[wasm_bindgen(js_name = "fromAddress")] from_address: String,
    #[wasm_bindgen(js_name = "toAddress")] to_address: String,
  ) -> Self {
    Self {
      transaction_type,
      currency,
      amount,
      decimals,
      from_address,
      to_address,
    }
  }
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

#[wasm_bindgen]
pub struct TransactionReferenceCodec;

#[wasm_bindgen]
impl TransactionReferenceCodec {
  #[wasm_bindgen(js_name = "contentType")]
  pub fn content_type() -> ContentTypeId {
    XmtpTransactionReferenceCodec::content_type().into()
  }

  #[wasm_bindgen]
  pub fn encode(transaction_reference: TransactionReference) -> Result<EncodedContent, JsError> {
    let encoded_content = XmtpTransactionReferenceCodec::encode(transaction_reference.into())
      .map_err(|e| JsError::new(&format!("{}", e)))?;
    Ok(encoded_content.into())
  }

  #[wasm_bindgen]
  pub fn decode(encoded_content: EncodedContent) -> Result<TransactionReference, JsError> {
    XmtpTransactionReferenceCodec::decode(encoded_content.into())
      .map(Into::into)
      .map_err(|e| JsError::new(&format!("{}", e)))
  }

  #[wasm_bindgen(js_name = "shouldPush")]
  pub fn should_push() -> bool {
    XmtpTransactionReferenceCodec::should_push()
  }
}
