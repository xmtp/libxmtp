use std::collections::HashMap;

use crate::{CodecError, ContentCodec};
use prost::Message;
use xmtp_proto::xmtp::mls::message_contents::{
    ContentTypeId, EncodedContent, content_types::MultiRemoteAttachment,
};

pub struct MultiRemoteAttachmentCodec {}

impl MultiRemoteAttachmentCodec {
    const AUTHORITY_ID: &'static str = "xmtp.org";
    pub const TYPE_ID: &'static str = "multiRemoteStaticAttachment";
    pub const MAJOR_VERSION: u32 = 1;
    pub const MINOR_VERSION: u32 = 0;
}

impl MultiRemoteAttachmentCodec {
    fn fallback(_: &MultiRemoteAttachment) -> Option<String> {
        Some(
            "Can't display this content. This app doesn't support multiple remote attachments."
                .to_string(),
        )
    }
}

impl ContentCodec<MultiRemoteAttachment> for MultiRemoteAttachmentCodec {
    fn content_type() -> ContentTypeId {
        ContentTypeId {
            authority_id: MultiRemoteAttachmentCodec::AUTHORITY_ID.to_string(),
            type_id: MultiRemoteAttachmentCodec::TYPE_ID.to_string(),
            version_major: MultiRemoteAttachmentCodec::MAJOR_VERSION,
            version_minor: MultiRemoteAttachmentCodec::MINOR_VERSION,
        }
    }

    fn encode(data: MultiRemoteAttachment) -> Result<EncodedContent, CodecError> {
        let mut buf = Vec::new();
        data.encode(&mut buf)
            .map_err(|e| CodecError::Encode(e.to_string()))?;

        Ok(EncodedContent {
            r#type: Some(MultiRemoteAttachmentCodec::content_type()),
            parameters: HashMap::new(),
            fallback: Self::fallback(&data),
            compression: None,
            content: buf,
        })
    }

    fn decode(content: EncodedContent) -> Result<MultiRemoteAttachment, CodecError> {
        let decoded = MultiRemoteAttachment::decode(content.content.as_slice())
            .map_err(|e| CodecError::Decode(e.to_string()))?;

        Ok(decoded)
    }

    fn should_push() -> bool {
        true
    }
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use xmtp_proto::xmtp::mls::message_contents::content_types::RemoteAttachmentInfo;

    use super::*;

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_encode_decode() {
        let attachment_info_1 = RemoteAttachmentInfo {
            content_digest: "0123456789abcdef".to_string(),
            secret: vec![0; 32],
            nonce: vec![0; 16],
            salt: vec![0; 16],
            scheme: "https".to_string(),
            url: "https://example.com/attachment".to_string(),
            content_length: Some(1000),
            filename: Some("attachment_1.jpg".to_string()),
        };
        let attachment_info_2 = RemoteAttachmentInfo {
            content_digest: "0123456789abcdef".to_string(),
            secret: vec![0; 32],
            nonce: vec![0; 16],
            salt: vec![0; 16],
            scheme: "https".to_string(),
            url: "https://example.com/attachment".to_string(),
            content_length: Some(1000),
            filename: Some("attachment_2.jpg".to_string()),
        };

        // Store the filenames before moving the attachment_info structs
        let filename_1 = attachment_info_1.filename.clone();
        let filename_2 = attachment_info_2.filename.clone();

        let new_multi_remote_attachment_data: MultiRemoteAttachment = MultiRemoteAttachment {
            attachments: vec![attachment_info_1.clone(), attachment_info_2.clone()],
        };

        let encoded = MultiRemoteAttachmentCodec::encode(new_multi_remote_attachment_data).unwrap();
        assert_eq!(
            encoded.clone().r#type.unwrap().type_id,
            "multiRemoteStaticAttachment"
        );
        assert!(!encoded.content.is_empty());

        let decoded = MultiRemoteAttachmentCodec::decode(encoded).unwrap();
        assert_eq!(decoded.attachments[0].filename, filename_1);
        assert_eq!(decoded.attachments[1].filename, filename_2);
        assert_eq!(decoded.attachments[0].content_length, Some(1000));
        assert_eq!(decoded.attachments[1].content_length, Some(1000));
    }
}
