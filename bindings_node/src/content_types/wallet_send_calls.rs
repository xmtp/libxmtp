use std::collections::HashMap;

use napi::bindgen_prelude::Result;
use napi_derive::napi;
use xmtp_content_types::{
  ContentCodec, wallet_send_calls::WalletSendCallsCodec as XmtpWalletSendCallsCodec,
};

use crate::{
  ErrorWrapper,
  encoded_content::{ContentTypeId, EncodedContent},
};

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
pub fn encode_wallet_send_calls(wallet_send_calls: WalletSendCalls) -> Result<EncodedContent> {
  let encoded_content =
    XmtpWalletSendCallsCodec::encode(wallet_send_calls.into()).map_err(ErrorWrapper::from)?;
  Ok(encoded_content.into())
}

#[napi]
pub fn decode_wallet_send_calls(encoded_content: EncodedContent) -> Result<WalletSendCalls> {
  Ok(
    XmtpWalletSendCallsCodec::decode(encoded_content.into())
      .map(Into::into)
      .map_err(ErrorWrapper::from)?,
  )
}

#[napi]
pub struct WalletSendCallsCodec {}

#[napi]
impl WalletSendCallsCodec {
  #[napi]
  pub fn content_type() -> ContentTypeId {
    XmtpWalletSendCallsCodec::content_type().into()
  }

  #[napi]
  pub fn encode(wallet_send_calls: WalletSendCalls) -> Result<EncodedContent> {
    let encoded_content =
      XmtpWalletSendCallsCodec::encode(wallet_send_calls.into()).map_err(ErrorWrapper::from)?;
    Ok(encoded_content.into())
  }

  #[napi]
  pub fn decode(encoded_content: EncodedContent) -> Result<WalletSendCalls> {
    Ok(
      XmtpWalletSendCallsCodec::decode(encoded_content.into())
        .map(Into::into)
        .map_err(ErrorWrapper::from)?,
    )
  }

  #[napi]
  pub fn should_push() -> bool {
    XmtpWalletSendCallsCodec::should_push()
  }
}
