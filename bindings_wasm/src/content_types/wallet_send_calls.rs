use crate::encoded_content::{ContentTypeId, EncodedContent};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tsify::Tsify;
use wasm_bindgen::{JsError, prelude::wasm_bindgen};
use xmtp_content_types::ContentCodec;
use xmtp_content_types::wallet_send_calls::{
  WalletSendCalls as XmtpWalletSendCalls, WalletSendCallsCodec as XmtpWalletSendCallsCodec,
};

#[derive(Clone, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
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

#[derive(Clone, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
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

#[derive(Clone, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
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

#[wasm_bindgen]
pub struct WalletSendCallsCodec;

#[wasm_bindgen]
impl WalletSendCallsCodec {
  #[wasm_bindgen(js_name = "contentType")]
  pub fn content_type() -> ContentTypeId {
    XmtpWalletSendCallsCodec::content_type().into()
  }

  #[wasm_bindgen]
  pub fn encode(
    #[wasm_bindgen(js_name = walletSendCalls)] wallet_send_calls: WalletSendCalls,
  ) -> Result<EncodedContent, JsError> {
    let encoded_content = XmtpWalletSendCallsCodec::encode(wallet_send_calls.into())
      .map_err(|e| JsError::new(&format!("{}", e)))?;
    Ok(encoded_content.into())
  }

  #[wasm_bindgen]
  pub fn decode(encoded_content: EncodedContent) -> Result<WalletSendCalls, JsError> {
    let wallet_send_calls = XmtpWalletSendCallsCodec::decode(encoded_content.into())
      .map_err(|e| JsError::new(&format!("{}", e)))?;
    Ok(wallet_send_calls.into())
  }

  #[wasm_bindgen(js_name = "shouldPush")]
  pub fn should_push() -> bool {
    XmtpWalletSendCallsCodec::should_push()
  }
}
