use std::collections::HashMap;

use prost::Message;

use super::{CodecError, ContentCodec};
use xmtp_proto::xmtp::mls::message_contents::content_types::LeaveRequest;
use xmtp_proto::xmtp::mls::message_contents::{ContentTypeId, EncodedContent};

pub struct LeaveRequestCodec {
    #[allow(dead_code)]
    authenticated_note: Option<Vec<u8>>,
}

impl LeaveRequestCodec {
    const AUTHORITY_ID: &'static str = "xmtp.org";
    pub const TYPE_ID: &'static str = "leave_request";
    pub const MAJOR_VERSION: u32 = 1;
    pub const MINOR_VERSION: u32 = 0;
}

impl ContentCodec<LeaveRequest> for LeaveRequestCodec {
    fn content_type() -> ContentTypeId {
        ContentTypeId {
            authority_id: LeaveRequestCodec::AUTHORITY_ID.to_string(),
            type_id: LeaveRequestCodec::TYPE_ID.to_string(),
            version_major: LeaveRequestCodec::MAJOR_VERSION,
            version_minor: LeaveRequestCodec::MINOR_VERSION,
        }
    }

    fn encode(data: LeaveRequest) -> Result<EncodedContent, CodecError> {
        let mut buf = Vec::new();
        data.encode(&mut buf)
            .map_err(|e| CodecError::Encode(e.to_string()))?;

        Ok(EncodedContent {
            r#type: Some(LeaveRequestCodec::content_type()),
            parameters: HashMap::new(),
            fallback: None,
            compression: None,
            content: buf,
        })
    }

    fn decode(content: EncodedContent) -> Result<LeaveRequest, CodecError> {
        let decoded = LeaveRequest::decode(content.content.as_slice())
            .map_err(|e| CodecError::Decode(e.to_string()))?;

        Ok(decoded)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);
    use super::*;

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_encode_decode() {
        let data = LeaveRequest {
            authenticated_note: None,
        };

        let encoded = LeaveRequestCodec::encode(data).unwrap();
        assert_eq!(encoded.clone().r#type.unwrap().type_id, "leave_request");

        let _ = LeaveRequestCodec::decode(encoded).unwrap();
    }
}
