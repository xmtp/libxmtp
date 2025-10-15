use std::collections::HashMap;

use crate::{CodecError, ContentCodec};
use serde::{Deserialize, Deserializer, Serialize};

use xmtp_proto::xmtp::mls::message_contents::{ContentTypeId, EncodedContent};

pub struct TransactionReferenceCodec {}

/// Legacy content type id at <https://github.com/xmtp/xmtp-js/blob/main/content-types/content-type-transaction-reference/src/TransactionReference.ts>
impl TransactionReferenceCodec {
    const AUTHORITY_ID: &'static str = "xmtp.org";
    pub const TYPE_ID: &'static str = "transactionReference";
    pub const MAJOR_VERSION: u32 = 1;
    pub const MINOR_VERSION: u32 = 0;
}

impl ContentCodec<TransactionReference> for TransactionReferenceCodec {
    fn content_type() -> ContentTypeId {
        ContentTypeId {
            authority_id: Self::AUTHORITY_ID.to_string(),
            type_id: Self::TYPE_ID.to_string(),
            version_major: Self::MAJOR_VERSION,
            version_minor: Self::MINOR_VERSION,
        }
    }

    fn encode(data: TransactionReference) -> Result<EncodedContent, CodecError> {
        let json = serde_json::to_vec(&data)
            .map_err(|e| CodecError::Encode(format!("JSON encode error: {e}")))?;

        Ok(EncodedContent {
            r#type: Some(Self::content_type()),
            parameters: HashMap::new(),
            fallback: Some(Self::fallback(&data)),
            compression: None,
            content: json,
        })
    }

    fn decode(encoded: EncodedContent) -> Result<TransactionReference, CodecError> {
        serde_json::from_slice(&encoded.content)
            .map_err(|e| CodecError::Decode(format!("JSON decode error: {e}")))
    }
}

impl TransactionReferenceCodec {
    fn fallback(content: &TransactionReference) -> String {
        if !content.reference.is_empty() {
            format!(
                "[Crypto transaction] Use a blockchain explorer to learn more using the transaction hash: {}",
                content.reference
            )
        } else {
            "Crypto transaction".to_string()
        }
    }
}

/// Custom deserializer for network_id that can handle both string and number
fn deserialize_network_id<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;

    // Use serde_json::Value to handle flexible deserialization
    let value = serde_json::Value::deserialize(deserializer)?;

    match value {
        serde_json::Value::String(s) => Ok(s),
        serde_json::Value::Number(n) => Ok(n.to_string()),
        _ => Err(Error::custom("networkId must be a string or number")),
    }
}

/// The main content type for transaction references
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TransactionReference {
    /// Optional namespace for the network (e.g., "eip155")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,

    /// Network ID (can be string or number in JSON)
    #[serde(rename = "networkId", deserialize_with = "deserialize_network_id")]
    pub network_id: String,

    /// Transaction hash
    pub reference: String,

    /// Optional metadata for the transaction
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<TransactionMetadata>,
}

/// Metadata attached to the transaction reference
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TransactionMetadata {
    #[serde(rename = "transactionType")]
    pub transaction_type: String,
    pub currency: String,
    pub amount: f64,
    pub decimals: u32,
    #[serde(rename = "fromAddress")]
    pub from_address: String,
    #[serde(rename = "toAddress")]
    pub to_address: String,
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);
    use super::*;

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_encode_decode_transaction_reference() {
        let tx = TransactionReference {
            namespace: Some("eip155".to_string()),
            network_id: "1".to_string(),
            reference: "0xabc123".to_string(),
            metadata: Some(TransactionMetadata {
                transaction_type: "payment".to_string(),
                currency: "ETH".to_string(),
                amount: 1.2345,
                decimals: 18,
                from_address: "0xsender".to_string(),
                to_address: "0xrecipient".to_string(),
            }),
        };

        let encoded = TransactionReferenceCodec::encode(tx.clone()).unwrap();
        let decoded = TransactionReferenceCodec::decode(encoded).unwrap();

        assert_eq!(decoded.reference, tx.reference);
        assert_eq!(
            decoded.metadata.as_ref().unwrap().currency,
            "ETH".to_string()
        );
    }
}
