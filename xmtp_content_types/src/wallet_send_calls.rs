use std::collections::HashMap;

use crate::{CodecError, ContentCodec};
use serde::{Deserialize, Serialize};
use xmtp_proto::xmtp::mls::message_contents::{ContentTypeId, EncodedContent};

pub struct WalletSendCallsCodec {}

impl WalletSendCallsCodec {
    const AUTHORITY_ID: &'static str = "xmtp.org";
    pub const TYPE_ID: &'static str = "walletSendCalls";
    pub const MAJOR_VERSION: u32 = 1;
    pub const MINOR_VERSION: u32 = 0;

    fn fallback(content: &WalletSendCalls) -> String {
        let json = serde_json::to_string(content).unwrap_or_else(|_| "{}".to_string());
        format!("[Transaction request generated]: {}", json)
    }
}

impl ContentCodec<WalletSendCalls> for WalletSendCallsCodec {
    fn content_type() -> ContentTypeId {
        ContentTypeId {
            authority_id: Self::AUTHORITY_ID.to_string(),
            type_id: Self::TYPE_ID.to_string(),
            version_major: Self::MAJOR_VERSION,
            version_minor: Self::MINOR_VERSION,
        }
    }

    fn encode(content: WalletSendCalls) -> Result<EncodedContent, CodecError> {
        let json = serde_json::to_vec(&content)
            .map_err(|e| CodecError::Encode(format!("JSON encode error: {e}")))?;

        Ok(EncodedContent {
            r#type: Some(Self::content_type()),
            parameters: HashMap::new(),
            fallback: Some(Self::fallback(&content)),
            compression: None,
            content: json,
        })
    }

    fn decode(encoded: EncodedContent) -> Result<WalletSendCalls, CodecError> {
        serde_json::from_slice(&encoded.content)
            .map_err(|e| CodecError::Decode(format!("JSON decode error: {e}")))
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WalletSendCalls {
    pub version: String,
    #[serde(rename = "chainId")]
    pub chain_id: String,
    pub from: String,
    pub calls: Vec<WalletCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<HashMap<String, String>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WalletCall {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gas: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<WalletCallMetadata>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WalletCallMetadata {
    pub description: String,
    #[serde(rename = "transactionType")]
    pub transaction_type: String,
    #[serde(flatten)]
    pub extra: HashMap<String, String>,
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use super::*;
    use crate::ContentCodec;

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_encode_decode_wallet_send_calls() {
        let params = WalletSendCalls {
            version: "1".to_string(),
            chain_id: "0x1".to_string(),
            from: "0xsender".to_string(),
            calls: vec![WalletCall {
                to: Some("0xrecipient".to_string()),
                data: Some("0xdeadbeef".to_string()),
                value: Some("0x0".to_string()),
                gas: Some("0x5208".to_string()),
                metadata: Some(WalletCallMetadata {
                    description: "Send funds".to_string(),
                    transaction_type: "transfer".to_string(),
                    extra: HashMap::from([("note".to_string(), "test".to_string())]),
                }),
            }],
            capabilities: Some(HashMap::from([("foo".to_string(), "bar".to_string())])),
        };

        let encoded = WalletSendCallsCodec::encode(params.clone()).unwrap();
        let decoded = WalletSendCallsCodec::decode(encoded).unwrap();

        assert_eq!(decoded.version, params.version);
        assert_eq!(decoded.chain_id, params.chain_id);
        assert_eq!(decoded.from, params.from);
        assert_eq!(decoded.calls.len(), 1);
        assert_eq!(
            decoded.calls[0].metadata.as_ref().unwrap().transaction_type,
            "transfer"
        );
    }
}
