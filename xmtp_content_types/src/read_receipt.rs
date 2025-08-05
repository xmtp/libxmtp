use std::collections::HashMap;

use crate::{CodecError, ContentCodec};
use serde::{Deserialize, Serialize};

use xmtp_proto::xmtp::mls::message_contents::{ContentTypeId, EncodedContent};

pub struct ReadReceiptCodec {}

/// Legacy content type id at https://github.com/xmtp/xmtp-js/blob/main/content-types/content-type-read-receipt/src/ReadReceipt.ts
impl ReadReceiptCodec {
    const AUTHORITY_ID: &'static str = "xmtp.org";
    pub const TYPE_ID: &'static str = "readReceipt";
}

impl ContentCodec<ReadReceipt> for ReadReceiptCodec {
    fn content_type() -> ContentTypeId {
        ContentTypeId {
            authority_id: Self::AUTHORITY_ID.to_string(),
            type_id: Self::TYPE_ID.to_string(),
            version_major: 1,
            version_minor: 0,
        }
    }

    fn encode(data: ReadReceipt) -> Result<EncodedContent, CodecError> {
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

    fn decode(encoded: EncodedContent) -> Result<ReadReceipt, CodecError> {
        serde_json::from_slice(&encoded.content)
            .map_err(|e| CodecError::Decode(format!("JSON decode error: {e}")))
    }
}

impl ReadReceiptCodec {
    fn fallback(content: &ReadReceipt) -> String {
        format!("[Read receipt for message {}]", content.reference)
    }
}

/// The main content type for read receipts
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ReadReceipt {
    /// The message ID that was read
    pub reference: String,

    /// The inbox ID of the user who sent the message that was read
    #[serde(rename = "referenceInboxId", skip_serializing_if = "Option::is_none")]
    pub reference_inbox_id: Option<String>,

    /// The timestamp when the message was read (in nanoseconds)
    pub read_at_ns: i64,
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);
    use super::*;

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_encode_decode_read_receipt() {
        let read_receipt = ReadReceipt {
            reference: "msg_123".to_string(),
            reference_inbox_id: Some("inbox_456".to_string()),
            read_at_ns: 1234567890,
        };

        let encoded = ReadReceiptCodec::encode(read_receipt.clone()).unwrap();
        let decoded = ReadReceiptCodec::decode(encoded).unwrap();

        assert_eq!(decoded.reference, read_receipt.reference);
        assert_eq!(decoded.reference_inbox_id, read_receipt.reference_inbox_id);
        assert_eq!(decoded.read_at_ns, read_receipt.read_at_ns);
    }
}
