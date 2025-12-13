use crate::encoded_content::EncodedContent;
use wasm_bindgen::{JsError, prelude::wasm_bindgen};
use xmtp_content_types::ContentCodec;
use xmtp_content_types::transaction_reference::TransactionReferenceCodec;

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone)]
pub struct TransactionReference {
  pub namespace: Option<String>,
  #[wasm_bindgen(js_name = "networkId")]
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

#[wasm_bindgen(js_name = "encodeTransactionReference")]
pub fn encode_transaction_reference(
  #[wasm_bindgen(js_name = "transactionReference")] transaction_reference: TransactionReference,
) -> Result<EncodedContent, JsError> {
  let encoded_content = TransactionReferenceCodec::encode(transaction_reference.into())
    .map_err(|e| JsError::new(&format!("{}", e)))?;
  Ok(encoded_content.into())
}

#[wasm_bindgen(js_name = "decodeTransactionReference")]
pub fn decode_transaction_reference(
  encoded_content: EncodedContent,
) -> Result<TransactionReference, JsError> {
  // Use TransactionReferenceCodec to decode into TransactionReference and convert to TransactionReference
  TransactionReferenceCodec::decode(encoded_content.into())
    .map(Into::into)
    .map_err(|e| JsError::new(&format!("{}", e)))
}
