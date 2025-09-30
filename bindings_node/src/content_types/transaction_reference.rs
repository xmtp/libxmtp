use napi_derive::napi;

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
      metadata: tr.metadata.map(|m| m.into()),
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
