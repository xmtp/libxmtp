use crate::{CodecError, ContentCodec};

use serde::{Deserialize, Serialize};
use xmtp_proto::xmtp::mls::message_contents::{ContentTypeId, EncodedContent};

// JSON format for reaction is defined here: https://github.com/xmtp/xmtp-js/blob/main/content-types/content-type-reaction/src/Reaction.ts
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Reaction {
    /// The action of the reaction ("added" or "removed")
    pub action: String,
    /// The message ID for the message that is being reacted to
    pub reference: String,
    /// The inbox ID of the user who sent the message that is being reacted to
    #[serde(rename = "referenceInboxId", skip_serializing_if = "Option::is_none")]
    pub reference_inbox_id: Option<String>,
    /// The schema of the content ("unicode", "shortcode", or "custom")
    pub schema: String,
    /// The content of the reaction
    pub content: String,
}

pub struct ReactionCodec {}

/// Content type id at <https://github.com/xmtp/xmtp-js/blob/main/content-types/content-type-reaction/src/Reaction.ts>
impl ReactionCodec {
    const AUTHORITY_ID: &'static str = "xmtp.org";
    pub const TYPE_ID: &'static str = "reaction";
    pub const MAJOR_VERSION: u32 = 1;
    pub const MINOR_VERSION: u32 = 0;
}

impl ContentCodec<Reaction> for ReactionCodec {
    fn content_type() -> ContentTypeId {
        ContentTypeId {
            authority_id: ReactionCodec::AUTHORITY_ID.to_string(),
            type_id: ReactionCodec::TYPE_ID.to_string(),
            version_major: ReactionCodec::MAJOR_VERSION,
            version_minor: ReactionCodec::MINOR_VERSION,
        }
    }

    fn encode(data: Reaction) -> Result<EncodedContent, CodecError> {
        let json = serde_json::to_string(&data).map_err(|e| CodecError::Encode(e.to_string()))?;
        Ok(EncodedContent {
            r#type: Some(ReactionCodec::content_type()),
            parameters: std::collections::HashMap::new(),
            fallback: None,
            compression: None,
            content: json.into_bytes(),
        })
    }

    fn decode(content: EncodedContent) -> Result<Reaction, CodecError> {
        let decoded = serde_json::from_slice::<Reaction>(&content.content)
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
    fn test_encode_decode_reaction() {
        let reference = "0123456789abcdef";
        let reaction = Reaction {
            reference: reference.to_string(),
            reference_inbox_id: Some("some_inbox_id".to_string()),
            action: "added".to_string(),
            content: "👍".to_string(),
            schema: "unicode".to_string(),
        };
        let encoded = ReactionCodec::encode(reaction.clone()).unwrap();
        let decoded = ReactionCodec::decode(encoded).unwrap();

        assert_eq!(decoded.reference, reference);
        assert_eq!(
            decoded.reference_inbox_id,
            Some("some_inbox_id".to_string())
        );
        assert_eq!(decoded.action, "added");
        assert_eq!(decoded.content, "👍");
        assert_eq!(decoded.schema, "unicode");
    }
}
