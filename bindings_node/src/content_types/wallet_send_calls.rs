use std::collections::HashMap;

use napi_derive::napi;

#[derive(Clone)]
#[napi(object)]
pub struct WalletSendCalls {
  pub version: String,
  pub chain_id: String,
  pub from: String,
  pub calls: Vec<WalletCall>,
  pub capabilities: Option<HashMap<String, String>>,
}

impl From<xmtp_content_types::wallet_send_calls::WalletSendCalls> for WalletSendCalls {
  fn from(wsc: xmtp_content_types::wallet_send_calls::WalletSendCalls) -> Self {
    Self {
      version: wsc.version,
      chain_id: wsc.chain_id,
      from: wsc.from,
      calls: wsc.calls.into_iter().map(|c| c.into()).collect(),
      capabilities: wsc.capabilities,
    }
  }
}

#[derive(Clone)]
#[napi(object)]
pub struct WalletCall {
  pub to: Option<String>,
  pub data: Option<String>,
  pub value: Option<String>,
  pub gas: Option<String>,
  pub metadata: Option<WalletCallMetadata>,
}

impl From<xmtp_content_types::wallet_send_calls::WalletCall> for WalletCall {
  fn from(call: xmtp_content_types::wallet_send_calls::WalletCall) -> Self {
    Self {
      to: call.to,
      data: call.data,
      value: call.value,
      gas: call.gas,
      metadata: call.metadata.map(|m| m.into()),
    }
  }
}

#[derive(Clone)]
#[napi(object)]
pub struct WalletCallMetadata {
  pub description: String,
  pub transaction_type: String,
  pub extra: HashMap<String, String>,
}

impl From<xmtp_content_types::wallet_send_calls::WalletCallMetadata> for WalletCallMetadata {
  fn from(meta: xmtp_content_types::wallet_send_calls::WalletCallMetadata) -> Self {
    Self {
      description: meta.description,
      transaction_type: meta.transaction_type,
      extra: meta.extra,
    }
  }
}
