use std::collections::HashMap;

use xmtp_proto::xmtp::mls::message_contents::{ContentTypeId, EncodedContent};

use super::{CodecError, ContentCodec};

pub struct TextCodec {}

impl TextCodec {
    const AUTHORITY_ID: &'static str = "xmtp.org";
    pub const TYPE_ID: &'static str = "text";
    const ENCODING_KEY: &'static str = "encoding";
    const ENCODING_UTF8: &'static str = "UTF-8";
    pub const MAJOR_VERSION: u32 = 1;
    pub const MINOR_VERSION: u32 = 0;
}

impl ContentCodec<String> for TextCodec {
    fn content_type() -> ContentTypeId {
        ContentTypeId {
            authority_id: TextCodec::AUTHORITY_ID.to_string(),
            type_id: TextCodec::TYPE_ID.to_string(),
            version_major: TextCodec::MAJOR_VERSION,
            version_minor: TextCodec::MINOR_VERSION,
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
                "Unsupported text encoding {encoding}"
            )));
        }
        let text = std::str::from_utf8(&content.content)
            .map_err(|utf8_err| CodecError::Decode(utf8_err.to_string()))?;
        Ok(text.to_string())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use crate::{text::TextCodec, ContentCodec};

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn can_encode_and_decode_text() {
        let text = "Hello, world!";
        let encoded_content =
            TextCodec::encode(text.to_string()).expect("Should encode successfully");
        let decoded_content =
            TextCodec::decode(encoded_content).expect("Should decode successfully");
        assert!(decoded_content == text);
    }
}
