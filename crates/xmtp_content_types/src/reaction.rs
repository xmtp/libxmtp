use crate::{
    CodecError, ContentCodec,
    compression::{DecompressionBudget, decompress_with_budget, decompress_with_limit},
};
use prost::Message;

use serde::{Deserialize, Serialize};
use xmtp_proto::xmtp::mls::message_contents::{
    ContentTypeId, EncodedContent,
    content_types::{ReactionAction, ReactionSchema, ReactionV2},
};

/// Largest `xmtp.org/reaction` content, after decompression, that the client
/// encodes or decodes. It applies to the `ReactionV2` bytes and to legacy JSON.
// implements: CTYPE-034
pub const MAX_REACTION_BYTES: usize = 4096;

fn check_reaction_size(len: usize) -> Result<(), String> {
    if len > MAX_REACTION_BYTES {
        return Err(format!(
            "reaction content is {len} bytes, more than {MAX_REACTION_BYTES}"
        ));
    }
    Ok(())
}

/// Decompress `content` before a codec reads it. `xmtp.org/reaction` content
/// of any major version stops at the reaction limit and fails when longer;
/// other content has the shared message budget.
// implements: CTYPE-024, CTYPE-025, CTYPE-034
pub fn decompress_with_reaction_limit(
    content: EncodedContent,
    budget: &mut DecompressionBudget,
) -> Result<EncodedContent, CodecError> {
    let reaction = content.r#type.as_ref().is_some_and(|kind| {
        kind.authority_id == ReactionCodec::AUTHORITY_ID && kind.type_id == ReactionCodec::TYPE_ID
    });
    if !reaction {
        return decompress_with_budget(content, budget);
    }
    let content = decompress_with_limit(content, budget, MAX_REACTION_BYTES)?;
    check_reaction_size(content.content.len()).map_err(CodecError::Decode)?;
    Ok(content)
}

pub struct ReactionCodec {}

/// Legacy content type id at <https://github.com/xmtp/xmtp-js/blob/main/content-types/content-type-reaction/src/Reaction.ts>
impl ReactionCodec {
    const AUTHORITY_ID: &'static str = "xmtp.org";
    pub const TYPE_ID: &'static str = "reaction";
    pub const MAJOR_VERSION: u32 = 2;
    pub const MINOR_VERSION: u32 = 0;
}

impl ReactionCodec {
    fn fallback(content: &ReactionV2) -> Option<String> {
        let (action, prepos) = if content.action == ReactionAction::Removed as i32 {
            ("Removed", "from")
        } else {
            ("Reacted with", "to")
        };
        Some(format!(
            "{} \"{}\" {} an earlier message",
            action, content.content, prepos
        ))
    }
}

impl ContentCodec<ReactionV2> for ReactionCodec {
    fn content_type() -> ContentTypeId {
        ContentTypeId {
            authority_id: ReactionCodec::AUTHORITY_ID.to_string(),
            type_id: ReactionCodec::TYPE_ID.to_string(),
            version_major: 2,
            version_minor: 0,
        }
    }

    fn encode(data: ReactionV2) -> Result<EncodedContent, CodecError> {
        let mut buf = Vec::new();
        data.encode(&mut buf)
            .map_err(|e| CodecError::Encode(e.to_string()))?;
        // implements: CTYPE-034
        check_reaction_size(buf.len()).map_err(CodecError::Encode)?;

        Ok(EncodedContent {
            r#type: Some(ReactionCodec::content_type()),
            parameters: Default::default(),
            fallback: Self::fallback(&data),
            compression: None,
            content: buf,
        })
    }

    fn decode(content: EncodedContent) -> Result<ReactionV2, CodecError> {
        // implements: CTYPE-034
        check_reaction_size(content.content.len()).map_err(CodecError::Decode)?;
        let decoded = ReactionV2::decode(content.content.as_slice())
            .map_err(|e| CodecError::Decode(e.to_string()))?;

        Ok(decoded)
    }

    fn should_push() -> bool {
        false
    }
}

// JSON format for legacy reaction is defined here: https://github.com/xmtp/xmtp-js/blob/main/content-types/content-type-reaction/src/Reaction.ts
#[derive(Debug, Serialize, Deserialize)]
pub struct LegacyReaction {
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

impl From<LegacyReaction> for ReactionV2 {
    fn from(legacy: LegacyReaction) -> Self {
        let action = match legacy.action.as_str() {
            "added" => ReactionAction::Added as i32,
            "removed" => ReactionAction::Removed as i32,
            _ => ReactionAction::Unspecified as i32,
        };

        let schema = match legacy.schema.as_str() {
            "unicode" => ReactionSchema::Unicode as i32,
            "shortcode" => ReactionSchema::Shortcode as i32,
            "custom" => ReactionSchema::Custom as i32,
            _ => ReactionSchema::Unspecified as i32,
        };

        ReactionV2 {
            reference: legacy.reference,
            reference_inbox_id: legacy.reference_inbox_id.unwrap_or_default(),
            action,
            content: legacy.content,
            schema,
        }
    }
}

impl LegacyReaction {
    pub fn decode(content: &[u8]) -> Option<LegacyReaction> {
        // Try to decode the content as UTF-8 string first
        if let Ok(decoded_content) = String::from_utf8(content.to_vec()) {
            // Try parsing as canonical JSON format
            if let Ok(reaction) = serde_json::from_str::<LegacyReaction>(&decoded_content) {
                return Some(reaction);
            }
            tracing::error!("legacy json deserialization failed");
        } else {
            tracing::error!("utf-8 deserialization failed");
        }
        None
    }
}

pub struct LegacyReactionCodec {}

/// Legacy content type id at <https://github.com/xmtp/xmtp-js/blob/main/content-types/content-type-reaction/src/Reaction.ts>
impl LegacyReactionCodec {
    const AUTHORITY_ID: &'static str = "xmtp.org";
    pub const TYPE_ID: &'static str = "reaction";
    pub const MAJOR_VERSION: u32 = 1;
    pub const MINOR_VERSION: u32 = 0;
}

impl LegacyReactionCodec {
    fn fallback(content: &LegacyReaction) -> Option<String> {
        let (action, prepos) = if content.action == "removed" {
            ("Removed", "from")
        } else {
            ("Reacted with", "to")
        };
        Some(format!(
            "{} \"{}\" {} an earlier message",
            action, content.content, prepos
        ))
    }
}

impl ContentCodec<LegacyReaction> for LegacyReactionCodec {
    fn content_type() -> ContentTypeId {
        ContentTypeId {
            authority_id: LegacyReactionCodec::AUTHORITY_ID.to_string(),
            type_id: LegacyReactionCodec::TYPE_ID.to_string(),
            version_major: LegacyReactionCodec::MAJOR_VERSION,
            version_minor: LegacyReactionCodec::MINOR_VERSION,
        }
    }

    fn encode(data: LegacyReaction) -> Result<EncodedContent, CodecError> {
        let json = serde_json::to_string(&data).map_err(|e| CodecError::Encode(e.to_string()))?;
        Ok(EncodedContent {
            r#type: Some(LegacyReactionCodec::content_type()),
            parameters: Default::default(),
            fallback: Self::fallback(&data),
            compression: None,
            content: json.into_bytes(),
        })
    }

    fn decode(content: EncodedContent) -> Result<LegacyReaction, CodecError> {
        // implements: CTYPE-034
        check_reaction_size(content.content.len()).map_err(CodecError::Decode)?;
        let decoded = serde_json::from_slice::<LegacyReaction>(&content.content)
            .map_err(|e| CodecError::Decode(e.to_string()))?;

        Ok(decoded)
    }

    fn should_push() -> bool {
        false
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use xmtp_proto::xmtp::mls::message_contents::content_types::{
        ReactionAction, ReactionSchema, ReactionV2,
    };

    use serde_json::json;
    use xmtp_common::rand_string;

    use super::*;

    #[cfg(not(target_arch = "wasm32"))]
    #[xmtp_common::test(unwrap_try = true)]
    async fn legacy_reaction_does_not_log_content_at_info() {
        use xmtp_logging::{Level, test_logging::LogCapture};

        let marker = "private-reaction-content-marker";
        let content = json!({
            "reference": "message-id",
            "action": "added",
            "content": marker,
            "schema": "custom"
        })
        .to_string();
        let capture = LogCapture::new(Level::Info);
        tracing::dispatcher::with_default(&capture.dispatch(), || {
            tracing::info!(target: "xmtp_content_types::reaction", "capture active");
            assert!(LegacyReaction::decode(content.as_bytes()).is_some());
        });
        let output = capture.output();
        assert!(output.contains("capture active"));
        assert!(
            !output.contains(marker),
            "reaction content reached Info logs"
        );
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    // verifies: CTYPE-007
    fn test_encode_decode() {
        let new_reaction_data = ReactionV2 {
            reference: rand_string::<24>(),
            reference_inbox_id: rand_string::<24>(),
            action: ReactionAction::Added as i32,
            content: "👍".to_string(),
            schema: ReactionSchema::Unicode as i32,
        };

        let encoded = ReactionCodec::encode(new_reaction_data).unwrap();
        assert_eq!(encoded.clone().r#type.unwrap().type_id, "reaction");
        assert!(!encoded.content.is_empty());

        let decoded = ReactionCodec::decode(encoded).unwrap();
        assert_eq!(decoded.action, ReactionAction::Added as i32);
        assert_eq!(decoded.content, "👍".to_string());
        assert_eq!(decoded.schema, ReactionSchema::Unicode as i32);
    }

    /// Reactions up to the limit round-trip. One byte more fails to encode and
    /// to decode, whichever field holds the bytes, and legacy JSON is limited too.
    // verifies: CTYPE-034
    #[xmtp_common::test(unwrap_try = true)]
    fn reaction_size_limit() {
        // `ReactionV2` bytes are one tag and a length varint, then `content`.
        let sized = |len: usize| {
            let mut reaction = ReactionV2 {
                content: String::new(),
                ..Default::default()
            };
            let content_len = len - reaction.encoded_len() - 3;
            reaction.content = "a".repeat(content_len);
            assert_eq!(reaction.encoded_len(), len);
            reaction
        };
        let at_limit = sized(MAX_REACTION_BYTES);
        let decoded = ReactionCodec::decode(ReactionCodec::encode(at_limit.clone())?)?;
        assert_eq!(decoded, at_limit);

        let over = sized(MAX_REACTION_BYTES + 1);
        assert!(matches!(
            ReactionCodec::encode(over.clone()),
            Err(CodecError::Encode(_))
        ));
        let encoded_over = EncodedContent {
            r#type: Some(ReactionCodec::content_type()),
            content: over.encode_to_vec(),
            ..Default::default()
        };
        assert!(matches!(
            ReactionCodec::decode(encoded_over),
            Err(CodecError::Decode(_))
        ));

        let long_inbox = ReactionV2 {
            reference_inbox_id: "a".repeat(MAX_REACTION_BYTES),
            content: "👍".into(),
            ..Default::default()
        };
        assert!(matches!(
            ReactionCodec::encode(long_inbox.clone()),
            Err(CodecError::Encode(_))
        ));
        let encoded_long_inbox = EncodedContent {
            r#type: Some(ReactionCodec::content_type()),
            content: long_inbox.encode_to_vec(),
            ..Default::default()
        };
        assert!(matches!(
            ReactionCodec::decode(encoded_long_inbox),
            Err(CodecError::Decode(_))
        ));

        let legacy = LegacyReactionCodec::encode(LegacyReaction {
            action: "added".into(),
            reference: "0102".into(),
            reference_inbox_id: None,
            schema: "custom".into(),
            content: "a".repeat(MAX_REACTION_BYTES),
        })?;
        assert!(legacy.content.len() > MAX_REACTION_BYTES);
        assert!(matches!(
            LegacyReactionCodec::decode(legacy),
            Err(CodecError::Decode(_))
        ));
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_legacy_reaction_deserialization() {
        let reference = "0123456789abcdef";
        let legacy_json = json!({
            "reference": reference,
            "referenceInboxId": "some_inbox_id",
            "action": "added",
            "content": "👍",
            "schema": "unicode"
        });

        let content = legacy_json.to_string().into_bytes();
        let decoded_reference: String = LegacyReaction::decode(&content).unwrap().reference;

        assert_eq!(decoded_reference, reference);

        // Test invalid JSON
        let invalid_content = b"invalid json";
        let failed_decode = LegacyReaction::decode(invalid_content);
        assert!(failed_decode.is_none());
    }
}
