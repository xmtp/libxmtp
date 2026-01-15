use crate::encoded_content::{ContentTypeId, EncodedContent};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tsify::Tsify;
use wasm_bindgen::JsError;
use wasm_bindgen::prelude::wasm_bindgen;
use xmtp_content_types::ContentCodec;
use xmtp_content_types::wallet_send_calls::WalletSendCalls as XmtpWalletSendCalls;
use xmtp_content_types::wallet_send_calls::WalletSendCallsCodec;

#[derive(Clone, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi, hashmap_as_object)]
#[serde(rename_all = "camelCase")]
pub struct WalletSendCalls {
  pub version: String,
  pub chain_id: String,
  pub from: String,
  pub calls: Vec<WalletCall>,
  #[tsify(optional)]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub capabilities: Option<HashMap<String, String>>,
}

impl TryFrom<XmtpWalletSendCalls> for WalletSendCalls {
  type Error = JsError;

  fn try_from(wsc: XmtpWalletSendCalls) -> Result<Self, Self::Error> {
    let calls: Result<Vec<_>, _> = wsc.calls.into_iter().map(TryInto::try_into).collect();
    Ok(Self {
      version: wsc.version,
      chain_id: wsc.chain_id,
      from: wsc.from,
      calls: calls?,
      capabilities: wsc.capabilities,
    })
  }
}

impl TryFrom<WalletSendCalls> for XmtpWalletSendCalls {
  type Error = JsError;

  fn try_from(wsc: WalletSendCalls) -> Result<Self, Self::Error> {
    let calls: Result<Vec<_>, _> = wsc.calls.into_iter().map(TryInto::try_into).collect();
    Ok(Self {
      version: wsc.version,
      chain_id: wsc.chain_id,
      from: wsc.from,
      calls: calls?,
      capabilities: wsc.capabilities,
    })
  }
}

#[derive(Clone, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi, hashmap_as_object)]
pub struct WalletCall {
  #[tsify(optional)]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub to: Option<String>,
  #[tsify(optional)]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub data: Option<String>,
  #[tsify(optional)]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub value: Option<String>,
  #[tsify(optional)]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub gas: Option<String>,
  #[tsify(optional)]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub metadata: Option<HashMap<String, String>>,
}

impl TryFrom<xmtp_content_types::wallet_send_calls::WalletCall> for WalletCall {
  type Error = JsError;

  fn try_from(
    call: xmtp_content_types::wallet_send_calls::WalletCall,
  ) -> Result<Self, Self::Error> {
    let metadata = call
      .metadata
      .map(|meta| serde_json::to_value(meta).and_then(serde_json::from_value))
      .transpose()
      .map_err(|e| JsError::new(&e.to_string()))?;
    Ok(Self {
      to: call.to,
      data: call.data,
      value: call.value,
      gas: call.gas,
      metadata,
    })
  }
}

impl TryFrom<WalletCall> for xmtp_content_types::wallet_send_calls::WalletCall {
  type Error = JsError;

  fn try_from(call: WalletCall) -> Result<Self, Self::Error> {
    let metadata = call
      .metadata
      .map(|meta| serde_json::to_value(meta).and_then(serde_json::from_value))
      .transpose()
      .map_err(|e| JsError::new(&e.to_string()))?;
    Ok(Self {
      to: call.to,
      data: call.data,
      value: call.value,
      gas: call.gas,
      metadata,
    })
  }
}

#[wasm_bindgen(js_name = "contentTypeWalletSendCalls")]
pub fn content_type_wallet_send_calls() -> ContentTypeId {
  WalletSendCallsCodec::content_type().into()
}

#[wasm_bindgen(js_name = "encodeWalletSendCalls")]
pub fn encode_wallet_send_calls(
  wallet_send_calls: WalletSendCalls,
) -> Result<EncodedContent, JsError> {
  let wsc: XmtpWalletSendCalls = wallet_send_calls.try_into()?;
  Ok(
    WalletSendCallsCodec::encode(wsc)
      .map_err(|e| JsError::new(&format!("{}", e)))?
      .into(),
  )
}
