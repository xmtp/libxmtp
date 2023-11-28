use std::collections::HashMap;

use xmtp_proto::xmtp::message_contents::{ContentTypeId, EncodedContent};

use super::{CodecError, ContentCodec};

pub struct TextCodec {}
impl TextCodec {
    const AUTHORITY_ID: &'static str = "xmtp.org";
    const TYPE_ID: &'static str = "text";
    const ENCODING_KEY: &'static str = "encoding";
    const ENCODING_UTF8: &'static str = "UTF-8";
}

impl ContentCodec<String> for TextCodec {
    fn content_type() -> ContentTypeId {
        ContentTypeId {
            authority_id: TextCodec::AUTHORITY_ID.to_string(),
            type_id: TextCodec::TYPE_ID.to_string(),
            version_major: 1,
            version_minor: 0,
        }
    }

    fn encode(text: String) -> Result<EncodedContent, CodecError> {
        Ok(EncodedContent {
            r#type: Some(TextCodec::content_type()),
            parameters: HashMap::from([(
                TextCodec::ENCODING_KEY.to_string(),
                TextCodec::ENCODING_UTF8.to_string(),
            )]),
            fallback: None,
            compression: None,
            content: text.into_bytes(),
        })
    }

    fn decode(content: EncodedContent) -> Result<String, CodecError> {
        let encoding = content
            .parameters
            .get(TextCodec::ENCODING_KEY)
            .map_or(TextCodec::ENCODING_UTF8, String::as_str);
        if encoding != TextCodec::ENCODING_UTF8 {
            return Err(CodecError::Decode(format!(
                "Unsupported text encoding {}",
                encoding
            )));
        }
        let text = std::str::from_utf8(&content.content)
            .map_err(|utf8_err| CodecError::Decode(utf8_err.to_string()))?;
        Ok(text.to_string())
    }
}

#[cfg(test)]
mod tests {
    use crate::codecs::{text::TextCodec, ContentCodec};

    #[test]
    fn can_encode_and_decode_text() {
        let text = "Hello, world!";
        let encoded_content =
            TextCodec::encode(text.to_string()).expect("Should encode successfully");
        let decoded_content =
            TextCodec::decode(encoded_content).expect("Should decode successfully");
        assert!(decoded_content == text);
    }
}
