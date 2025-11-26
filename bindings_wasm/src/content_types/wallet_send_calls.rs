use js_sys::Uint8Array;
use prost::Message;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use wasm_bindgen::JsValue;
use wasm_bindgen::{JsError, prelude::wasm_bindgen};
use xmtp_content_types::ContentCodec;
use xmtp_content_types::wallet_send_calls::{
  WalletSendCalls as XmtpWalletSendCalls, WalletSendCallsCodec,
};
use xmtp_proto::xmtp::mls::message_contents::EncodedContent;

#[derive(Clone, Serialize, Deserialize)]
pub struct WalletSendCalls {
  pub version: String,
  pub chain_id: String,
  pub from: String,
  pub calls: Vec<WalletCall>,
  pub capabilities: Option<HashMap<String, String>>,
}

impl From<XmtpWalletSendCalls> for WalletSendCalls {
  fn from(wsc: XmtpWalletSendCalls) -> Self {
    Self {
      version: wsc.version,
      chain_id: wsc.chain_id,
      from: wsc.from,
      calls: wsc.calls.into_iter().map(Into::into).collect(),
      capabilities: wsc.capabilities,
    }
  }
}

impl From<WalletSendCalls> for XmtpWalletSendCalls {
  fn from(wsc: WalletSendCalls) -> Self {
    Self {
      version: wsc.version,
      chain_id: wsc.chain_id,
      from: wsc.from,
      calls: wsc.calls.into_iter().map(Into::into).collect(),
      capabilities: wsc.capabilities,
    }
  }
}

#[derive(Clone, Serialize, Deserialize)]
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
      metadata: call.metadata.map(Into::into),
    }
  }
}

impl From<WalletCall> for xmtp_content_types::wallet_send_calls::WalletCall {
  fn from(call: WalletCall) -> Self {
    Self {
      to: call.to,
      data: call.data,
      value: call.value,
      gas: call.gas,
      metadata: call.metadata.map(Into::into),
    }
  }
}

#[derive(Clone, Serialize, Deserialize)]
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

impl From<WalletCallMetadata> for xmtp_content_types::wallet_send_calls::WalletCallMetadata {
  fn from(meta: WalletCallMetadata) -> Self {
    Self {
      description: meta.description,
      transaction_type: meta.transaction_type,
      extra: meta.extra,
    }
  }
}

#[wasm_bindgen(js_name = "encodeWalletSendCalls")]
pub fn encode_wallet_send_calls(wallet_send_calls: JsValue) -> Result<Uint8Array, JsError> {
  let wallet_send_calls: WalletSendCalls = serde_wasm_bindgen::from_value(wallet_send_calls)
    .map_err(|e| JsError::new(&format!("{}", e)))?;

  // Use WalletSendCallsCodec to encode the wallet send calls
  let encoded = WalletSendCallsCodec::encode(wallet_send_calls.into())
    .map_err(|e| JsError::new(&format!("{}", e)))?;

  // Encode the EncodedContent to bytes
  let mut buf = Vec::new();
  encoded
    .encode(&mut buf)
    .map_err(|e| JsError::new(&format!("{}", e)))?;

  Ok(Uint8Array::from(buf.as_slice()))
}

#[wasm_bindgen(js_name = "decodeWalletSendCalls")]
pub fn decode_wallet_send_calls(bytes: Uint8Array) -> Result<JsValue, JsError> {
  // Decode bytes into EncodedContent
  let encoded_content = EncodedContent::decode(bytes.to_vec().as_slice())
    .map_err(|e| JsError::new(&format!("{}", e)))?;

  // Use WalletSendCallsCodec to decode into WalletSendCalls and convert to WalletSendCalls
  let wallet_send_calls =
    WalletSendCallsCodec::decode(encoded_content).map_err(|e| JsError::new(&format!("{}", e)))?;
  let wallet_send_calls: WalletSendCalls = wallet_send_calls.into();

  serde_wasm_bindgen::to_value(&wallet_send_calls).map_err(|e| JsError::new(&format!("{}", e)))
}
