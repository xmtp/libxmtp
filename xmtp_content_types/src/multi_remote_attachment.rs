use std::collections::HashMap;

use crate::{
    bytes_to_encoded_content, encoded_content_to_bytes,
    encryption::{encrypt_encoded_content, EncryptedEncodedContent},
    CodecError, ContentCodec,
};
use libsecp256k1::{PublicKey, SecretKey};
use prost::Message;
use xmtp_proto::xmtp::mls::message_contents::{
    content_types::MultiRemoteAttachment, ContentTypeId, EncodedContent,
};

pub struct MultiRemoteAttachmentCodec {}

impl MultiRemoteAttachmentCodec {
    const AUTHORITY_ID: &'static str = "xmtp.org";
    pub const TYPE_ID: &'static str = "multiRemoteStaticAttachment";
}

impl ContentCodec<MultiRemoteAttachment> for MultiRemoteAttachmentCodec {
    fn content_type() -> ContentTypeId {
        ContentTypeId {
            authority_id: MultiRemoteAttachmentCodec::AUTHORITY_ID.to_string(),
            type_id: MultiRemoteAttachmentCodec::TYPE_ID.to_string(),
            version_major: 1,
            version_minor: 0,
        }
    }

    fn encode(data: MultiRemoteAttachment) -> Result<EncodedContent, CodecError> {
        let mut buf = Vec::new();
        data.encode(&mut buf)
            .map_err(|e| CodecError::Encode(e.to_string()))?;

        Ok(EncodedContent {
            r#type: Some(MultiRemoteAttachmentCodec::content_type()),
            parameters: HashMap::new(),
            fallback: Some(
                "Can’t display. This app doesn’t support multi remote attachments.".to_string(),
            ),
            compression: None,
            content: buf,
        })
    }

    fn decode(content: EncodedContent) -> Result<MultiRemoteAttachment, CodecError> {
        let decoded = MultiRemoteAttachment::decode(content.content.as_slice())
            .map_err(|e| CodecError::Decode(e.to_string()))?;

        Ok(decoded)
    }
}

pub struct EncryptedMultiRemoteAttachmentPreUpload {
    pub secret: Vec<u8>,
    pub attachments: Vec<EncryptedEncodedContent>,
}

impl TryFrom<Vec<Vec<u8>>> for EncryptedMultiRemoteAttachmentPreUpload {
    type Error = CodecError;

    fn try_from(attachments: Vec<Vec<u8>>) -> Result<Self, Self::Error> {
        let secret_key = SecretKey::random(&mut rand::thread_rng());
        let public_key = PublicKey::from_secret_key(&secret_key);

        let attachments: Vec<EncodedContent> = attachments
            .into_iter()
            .map(bytes_to_encoded_content)
            .collect();

        let encrypted_attachments = attachments
            .into_iter()
            .map(|attachment| {
                encrypt_encoded_content(
                    &secret_key.serialize(),
                    &public_key.serialize(),
                    attachment,
                )
                .unwrap()
            })
            .collect();

        Ok(EncryptedMultiRemoteAttachmentPreUpload {
            secret: secret_key.serialize().to_vec(),
            attachments: encrypted_attachments,
        })
    }
}
impl EncryptedMultiRemoteAttachmentPreUpload {
    pub fn try_into_bytes(self) -> Result<Vec<Vec<u8>>, CodecError> {
        // Reconstruct keys from the stored secret
        let secret_key_bytes: [u8; 32] = self
            .secret
            .try_into()
            .map_err(|_| CodecError::Decode("Secret key must be exactly 32 bytes".to_string()))?;

        let secret_key = SecretKey::parse(&secret_key_bytes)
            .map_err(|e| CodecError::Decode(format!("Failed to parse secret key: {}", e)))?;
        let public_key = PublicKey::from_secret_key(&secret_key);

        // Decrypt each attachment
        self.attachments
            .into_iter()
            .map(|encrypted_attachment| {
                let decoded_content = crate::encryption::decrypt_encoded_content(
                    &secret_key.serialize(),
                    &public_key.serialize(),
                    encrypted_attachment,
                )
                .map_err(CodecError::Decode)?;
                // Extract the raw bytes from the decoded content
                Ok(encoded_content_to_bytes(decoded_content))
            })
            .collect()
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
            nonce: vec![0; 16],
            salt: vec![0; 16],
            scheme: "https".to_string(),
            url: "https://example.com/attachment".to_string(),
            filename: "attachment_1.jpg".to_string(),
        };
        let attachment_info_2 = RemoteAttachmentInfo {
            content_digest: "0123456789abcdef".to_string(),
            nonce: vec![0; 16],
            salt: vec![0; 16],
            scheme: "https".to_string(),
            url: "https://example.com/attachment".to_string(),
            filename: "attachment_2.jpg".to_string(),
        };

        // Store the filenames before moving the attachment_info structs
        let filename_1 = attachment_info_1.filename.clone();
        let filename_2 = attachment_info_2.filename.clone();

        let new_multi_remote_attachment_data: MultiRemoteAttachment = MultiRemoteAttachment {
            secret: vec![0; 32],
            attachments: vec![attachment_info_1.clone(), attachment_info_2.clone()],
            num_attachments: Some(2),
            max_attachment_content_length: Some(1000),
        };

        let encoded = MultiRemoteAttachmentCodec::encode(new_multi_remote_attachment_data).unwrap();
        assert_eq!(
            encoded.clone().r#type.unwrap().type_id,
            "multiRemoteStaticAttachment"
        );
        assert!(!encoded.content.is_empty());

        let decoded = MultiRemoteAttachmentCodec::decode(encoded).unwrap();
        assert_eq!(decoded.secret, vec![0; 32]);
        assert_eq!(decoded.attachments[0].filename, filename_1);
        assert_eq!(decoded.attachments[1].filename, filename_2);
        assert_eq!(decoded.num_attachments, Some(2));
        assert_eq!(decoded.max_attachment_content_length, Some(1000));
    }
}
