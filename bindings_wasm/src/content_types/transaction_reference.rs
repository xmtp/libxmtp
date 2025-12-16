use crate::encoded_content::{ContentTypeId, EncodedContent};
use serde::{Deserialize, Serialize};
use tsify::Tsify;
use wasm_bindgen::JsError;
use wasm_bindgen::prelude::wasm_bindgen;
use xmtp_content_types::ContentCodec;
use xmtp_content_types::transaction_reference::TransactionReferenceCodec;

#[derive(Clone, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct TransactionReference {
  #[tsify(optional)]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub namespace: Option<String>,
  pub network_id: String,
  pub reference: String,
  #[tsify(optional)]
  #[serde(skip_serializing_if = "Option::is_none")]
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

#[derive(Clone, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
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

#[wasm_bindgen(js_name = "transactionReferenceContentType")]
pub fn transaction_reference_content_type() -> ContentTypeId {
  TransactionReferenceCodec::content_type().into()
}

#[wasm_bindgen(js_name = "encodeTransactionReference")]
pub fn encode_transaction_reference(
  transaction_reference: TransactionReference,
) -> Result<EncodedContent, JsError> {
  Ok(
    TransactionReferenceCodec::encode(transaction_reference.into())
      .map_err(|e| JsError::new(&format!("{}", e)))?
      .into(),
  )
}
