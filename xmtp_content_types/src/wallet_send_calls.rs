pub struct WalletSendCallsCodec;

impl WalletSendCallsCodec {
    const AUTHORITY_ID: &'static str = "xmtp.org";
    const TYPE_ID: &'static str = "walletSendCalls";
}

impl ContentCodec<WalletSendCalls> for WalletSendCallsCodec {
    fn content_type() -> ContentTypeId {
        ContentTypeId {
            authority_id: Self::AUTHORITY_ID.to_string(),
            type_id: Self::TYPE_ID.to_string(),
            version_major: 1,
            version_minor: 0,
        }
    }

    fn encode(data: WalletSendCalls) -> Result<EncodedContent, CodecError> {
        let json = serde_json::to_vec(&data)
            .map_err(|e| CodecError::Encode(format!("json encode: {}", e)))?;

        Ok(EncodedContent {
            r#type: Some(Self::content_type()),
            parameters: HashMap::new(),
            fallback: Some(format!("[Transaction request generated]: {}", String::from_utf8_lossy(&json))),
            compression: None,
            content: json,
        })
    }

    fn decode(encoded: EncodedContent) -> Result<WalletSendCalls, CodecError> {
        serde_json::from_slice(&encoded.content)
            .map_err(|e| CodecError::Decode(format!("json decode: {}", e)))
    }
}
