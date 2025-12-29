use std::collections::HashMap;

use napi::bindgen_prelude::{Error, Result};
use napi_derive::napi;
use xmtp_content_types::{ContentCodec, wallet_send_calls::WalletSendCallsCodec};

use crate::ErrorWrapper;
use crate::encoded_content::{ContentTypeId, EncodedContent};

#[derive(Clone)]
#[napi(object)]
pub struct WalletSendCalls {
  pub version: String,
  pub chain_id: String,
  pub from: String,
  pub calls: Vec<WalletCall>,
  pub capabilities: Option<HashMap<String, String>>,
}

impl TryFrom<xmtp_content_types::wallet_send_calls::WalletSendCalls> for WalletSendCalls {
  type Error = Error;

  fn try_from(
    wsc: xmtp_content_types::wallet_send_calls::WalletSendCalls,
  ) -> std::result::Result<Self, Self::Error> {
    let calls: std::result::Result<Vec<_>, _> =
      wsc.calls.into_iter().map(TryInto::try_into).collect();
    Ok(Self {
      version: wsc.version,
      chain_id: wsc.chain_id,
      from: wsc.from,
      calls: calls?,
      capabilities: wsc.capabilities,
    })
  }
}

impl TryFrom<WalletSendCalls> for xmtp_content_types::wallet_send_calls::WalletSendCalls {
  type Error = Error;

  fn try_from(wsc: WalletSendCalls) -> std::result::Result<Self, Self::Error> {
    let calls: std::result::Result<Vec<_>, _> =
      wsc.calls.into_iter().map(TryInto::try_into).collect();
    Ok(Self {
      version: wsc.version,
      chain_id: wsc.chain_id,
      from: wsc.from,
      calls: calls?,
      capabilities: wsc.capabilities,
    })
  }
}

#[derive(Clone)]
#[napi(object)]
pub struct WalletCall {
  pub to: Option<String>,
  pub data: Option<String>,
  pub value: Option<String>,
  pub gas: Option<String>,
  pub metadata: Option<HashMap<String, String>>,
}

impl TryFrom<xmtp_content_types::wallet_send_calls::WalletCall> for WalletCall {
  type Error = Error;

  fn try_from(
    call: xmtp_content_types::wallet_send_calls::WalletCall,
  ) -> std::result::Result<Self, Self::Error> {
    let metadata = call
      .metadata
      .map(|meta| serde_json::to_value(meta).and_then(serde_json::from_value))
      .transpose()
      .map_err(|e| Error::from_reason(e.to_string()))?;
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
  type Error = Error;

  fn try_from(call: WalletCall) -> std::result::Result<Self, Self::Error> {
    let metadata = call
      .metadata
      .map(|meta| serde_json::to_value(meta).and_then(serde_json::from_value))
      .transpose()
      .map_err(|e| Error::from_reason(e.to_string()))?;
    Ok(Self {
      to: call.to,
      data: call.data,
      value: call.value,
      gas: call.gas,
      metadata,
    })
  }
}

#[napi]
pub fn content_type_wallet_send_calls() -> ContentTypeId {
  WalletSendCallsCodec::content_type().into()
}

#[napi]
pub fn encode_wallet_send_calls(wallet_send_calls: WalletSendCalls) -> Result<EncodedContent> {
  let wsc = wallet_send_calls.try_into()?;
  Ok(
    WalletSendCallsCodec::encode(wsc)
      .map_err(ErrorWrapper::from)?
      .into(),
  )
}
