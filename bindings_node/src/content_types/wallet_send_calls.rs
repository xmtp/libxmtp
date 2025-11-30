use std::collections::HashMap;

use napi::bindgen_prelude::{Result, Uint8Array};
use napi_derive::napi;
use prost::Message;
use xmtp_content_types::{ContentCodec, wallet_send_calls::WalletSendCallsCodec};
use xmtp_proto::xmtp::mls::message_contents::EncodedContent;

use crate::ErrorWrapper;

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
      calls: wsc.calls.into_iter().map(Into::into).collect(),
      capabilities: wsc.capabilities,
    }
  }
}

impl From<WalletSendCalls> for xmtp_content_types::wallet_send_calls::WalletSendCalls {
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

impl From<WalletCallMetadata> for xmtp_content_types::wallet_send_calls::WalletCallMetadata {
  fn from(meta: WalletCallMetadata) -> Self {
    Self {
      description: meta.description,
      transaction_type: meta.transaction_type,
      extra: meta.extra,
    }
  }
}

#[napi]
pub fn encode_wallet_send_calls(wallet_send_calls: WalletSendCalls) -> Result<Uint8Array> {
  // Use WalletSendCallsCodec to encode the wallet send calls
  let encoded =
    WalletSendCallsCodec::encode(wallet_send_calls.into()).map_err(ErrorWrapper::from)?;

  // Encode the EncodedContent to bytes
  let mut buf = Vec::new();
  encoded.encode(&mut buf).map_err(ErrorWrapper::from)?;

  Ok(buf.into())
}

#[napi]
pub fn decode_wallet_send_calls(bytes: Uint8Array) -> Result<WalletSendCalls> {
  // Decode bytes into EncodedContent
  let encoded_content = EncodedContent::decode(bytes.as_ref()).map_err(ErrorWrapper::from)?;

  // Use WalletSendCallsCodec to decode into WalletSendCalls and convert to WalletSendCalls
  Ok(
    WalletSendCallsCodec::decode(encoded_content)
      .map(Into::into)
      .map_err(ErrorWrapper::from)?,
  )
}
