use std::collections::HashMap;

use xmtp_proto::xmtp::mls::message_contents::{ContentTypeId, EncodedContent};

use super::{CodecError, ContentCodec};

pub struct MarkdownCodec {}

impl MarkdownCodec {
    const AUTHORITY_ID: &'static str = "xmtp.org";
    pub const TYPE_ID: &'static str = "markdown";
    const ENCODING_KEY: &'static str = "encoding";
    const ENCODING_UTF8: &'static str = "UTF-8";
    pub const MAJOR_VERSION: u32 = 1;
    pub const MINOR_VERSION: u32 = 0;
}

impl ContentCodec<String> for MarkdownCodec {
    fn content_type() -> ContentTypeId {
        ContentTypeId {
            authority_id: MarkdownCodec::AUTHORITY_ID.to_string(),
            type_id: MarkdownCodec::TYPE_ID.to_string(),
            version_major: MarkdownCodec::MAJOR_VERSION,
            version_minor: MarkdownCodec::MINOR_VERSION,
        }
    }

    fn encode(markdown: String) -> Result<EncodedContent, CodecError> {
        Ok(EncodedContent {
            r#type: Some(MarkdownCodec::content_type()),
            parameters: HashMap::from([(
                MarkdownCodec::ENCODING_KEY.to_string(),
                MarkdownCodec::ENCODING_UTF8.to_string(),
            )]),
            fallback: None,
            compression: None,
            content: markdown.into_bytes(),
        })
    }

    fn decode(content: EncodedContent) -> Result<String, CodecError> {
        let encoding = content
            .parameters
            .get(MarkdownCodec::ENCODING_KEY)
            .map_or(MarkdownCodec::ENCODING_UTF8, String::as_str);
        if encoding != MarkdownCodec::ENCODING_UTF8 {
            return Err(CodecError::Decode(format!(
                "Unsupported text encoding {encoding}"
            )));
        }
        let markdown = std::str::from_utf8(&content.content)
            .map_err(|utf8_err| CodecError::Decode(utf8_err.to_string()))?;
        Ok(markdown.to_string())
    }

    fn should_push() -> bool {
        true
    }
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use crate::{ContentCodec, markdown::MarkdownCodec};

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn can_encode_and_decode_markdown() {
        let markdown = "# Hello, world!";
        let encoded_content =
            MarkdownCodec::encode(markdown.to_string()).expect("Should encode successfully");
        let decoded_content =
            MarkdownCodec::decode(encoded_content).expect("Should decode successfully");
        assert!(decoded_content == markdown);
    }
}
