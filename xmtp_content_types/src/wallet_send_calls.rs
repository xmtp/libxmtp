use std::collections::HashMap;

use prost::Message;

use xmtp_proto::xmtp::mls::message_contents::{
    content_types::WalletSendCalls, ContentTypeId, EncodedContent,
};

use super::{CodecError, ContentCodec};

pub struct WalletSendCallsCodec {}

impl WalletSendCallsCodec {
    const AUTHORITY_ID: &'static str = "xmtp.org";
    pub const TYPE_ID: &'static str = "walletSendCalls";
}

impl ContentCodec<WalletSendCalls> for WalletSendCallsCodec {
    fn content_type() -> ContentTypeId {
        ContentTypeId {
            authority_id: WalletSendCallsCodec::AUTHORITY_ID.to_string(),
            type_id: WalletSendCallsCodec::TYPE_ID.to_string(),
            version_major: 1,
            version_minor: 0,
        }
    }

    fn encode(data: WalletSendCalls) -> Result<EncodedContent, CodecError> {
        let mut buf = Vec::new();
        data.encode(&mut buf)
            .map_err(|e| CodecError::Encode(e.to_string()))?;

        Ok(EncodedContent {
            r#type: Some(WalletSendCallsCodec::content_type()),
            parameters: HashMap::new(),
            fallback: None,
            compression: None,
            content: buf,
        })
    }

    fn decode(content: EncodedContent) -> Result<WalletSendCalls, CodecError> {
        let decoded = WalletSendCalls::decode(content.content.as_slice())
            .map_err(|e| CodecError::Decode(e.to_string()))?;

        Ok(decoded)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use super::*;
    use xmtp_proto::xmtp::mls::message_contents::content_types::{Call, WalletSendCalls};

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_encode_decode() {
        let mut metadata = HashMap::new();
        metadata.insert("foo".to_string(), "bar".to_string());

        let mut capabilities = HashMap::new();
        capabilities.insert("foo".to_string(), "bar".to_string());

        let data = WalletSendCalls {
            version: "1".to_string(),
            chain_id: "0x1".to_string(),
            from: "0x123".to_string(),
            calls: vec![Call {
                to: "0x123".to_string(),
                data: "0x123".to_string(),
                value: "0x123".to_string(),
                gas: "0x123".to_string(),
                metadata,
            }],
            capabilities,
        };

        let encoded = WalletSendCallsCodec::encode(data).unwrap();
        assert_eq!(encoded.clone().r#type.unwrap().type_id, "walletSendCalls");
        assert!(!encoded.content.is_empty());

        let decoded = WalletSendCallsCodec::decode(encoded).unwrap();
        assert_eq!(decoded.version, "1");
        assert_eq!(decoded.chain_id, "0x1");
        assert_eq!(decoded.from, "0x123");
        assert_eq!(decoded.calls.len(), 1);
        let call = &decoded.calls[0];
        assert_eq!(call.to, "0x123");
        assert_eq!(call.data, "0x123");
        assert_eq!(call.value, "0x123");
        assert_eq!(call.gas, "0x123");
        assert_eq!(call.metadata.get("foo").unwrap(), "bar");
        assert_eq!(decoded.capabilities.get("foo").unwrap(), "bar");
    }
}
