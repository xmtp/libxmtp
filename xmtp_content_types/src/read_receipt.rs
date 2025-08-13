use std::collections::HashMap;

use crate::{CodecError, ContentCodec};
use serde::{Deserialize, Serialize};

use xmtp_proto::xmtp::mls::message_contents::{ContentTypeId, EncodedContent};

pub struct ReadReceiptCodec {}

/// Legacy content type id at https://github.com/xmtp/xmtp-js/blob/main/content-types/content-type-read-receipt/src/ReadReceipt.ts
impl ReadReceiptCodec {
    const AUTHORITY_ID: &'static str = "xmtp.org";
    pub const TYPE_ID: &'static str = "readReceipt";
    pub const MAJOR_VERSION: u32 = 1;
    pub const MINOR_VERSION: u32 = 0;
}

impl ContentCodec<ReadReceipt> for ReadReceiptCodec {
    fn content_type() -> ContentTypeId {
        ContentTypeId {
            authority_id: Self::AUTHORITY_ID.to_string(),
            type_id: Self::TYPE_ID.to_string(),
            version_major: ReadReceiptCodec::MAJOR_VERSION,
            version_minor: ReadReceiptCodec::MINOR_VERSION,
        }
    }

    fn encode(_: ReadReceipt) -> Result<EncodedContent, CodecError> {
        Ok(EncodedContent {
            r#type: Some(Self::content_type()),
            parameters: HashMap::new(),
            fallback: None,
            compression: None,
            content: vec![],
        })
    }

    fn decode(_: EncodedContent) -> Result<ReadReceipt, CodecError> {
        Ok(ReadReceipt {})
    }
}

/// The main content type for read receipts
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ReadReceipt {}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);
    use super::*;

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_encode_decode_read_receipt() {
        let read_receipt = ReadReceipt {};

        let encoded = ReadReceiptCodec::encode(read_receipt.clone()).unwrap();
        ReadReceiptCodec::decode(encoded).unwrap();
    }
}
