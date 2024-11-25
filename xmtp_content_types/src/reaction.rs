use std::collections::HashMap;

use prost::Message;

use xmtp_proto::xmtp::mls::message_contents::{ContentTypeId, EncodedContent};
use xmtp_proto::xmtp::reactions::Reaction;

use super::{CodecError, ContentCodec};

pub struct ReactionCodec {}

impl ReactionCodec {
    const AUTHORITY_ID: &'static str = "xmtp.org";
    const TYPE_ID: &'static str = "reaction";
}

impl ContentCodec<Reaction> for ReactionCodec {
    fn content_type() -> ContentTypeId {
        ContentTypeId {
            authority_id: ReactionCodec::AUTHORITY_ID.to_string(),
            type_id: ReactionCodec::TYPE_ID.to_string(),
            version_major: 1,
            version_minor: 0,
        }
    }

    fn encode(data: Reaction) -> Result<EncodedContent, CodecError> {
        let mut buf = Vec::new();
        data.encode(&mut buf)
            .map_err(|e| CodecError::Encode(e.to_string()))?;

        Ok(EncodedContent {
            r#type: Some(ReactionCodec::content_type()),
            parameters: HashMap::new(),
            fallback: None,
            compression: None,
            content: buf,
        })
    }

    fn decode(content: EncodedContent) -> Result<Reaction, CodecError> {
        let decoded = Reaction::decode(content.content.as_slice())
            .map_err(|e| CodecError::Decode(e.to_string()))?;

        Ok(decoded)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use xmtp_proto::xmtp::reactions::{ReactionAction, ReactionSchema};

    use crate::test_utils::rand_string;

    use super::*;

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_encode_decode() {
        let new_reaction_data = Reaction {
            reference: rand_string(),
            reference_inbox_id: rand_string(),
            action: ReactionAction::ActionAdded as i32,
            content: "👍".to_string(),
            schema: ReactionSchema::SchemaUnicode as i32,
        };

        let encoded = ReactionCodec::encode(new_reaction_data).unwrap();
        assert_eq!(encoded.clone().r#type.unwrap().type_id, "reaction");
        assert!(!encoded.content.is_empty());

        let decoded = ReactionCodec::decode(encoded).unwrap();
        assert_eq!(decoded.action, ReactionAction::ActionAdded as i32);
        assert_eq!(decoded.content, "👍".to_string());
        assert_eq!(decoded.schema, ReactionSchema::SchemaUnicode as i32);
    }
}
