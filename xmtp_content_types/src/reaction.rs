use std::collections::HashMap;

use crate::{CodecError, ContentCodec};
use prost::Message;

use xmtp_proto::xmtp::mls::message_contents::{
    content_types::ReactionV2, ContentTypeId, EncodedContent,
};

pub struct ReactionCodec {}

/// Legacy content type id at https://github.com/xmtp/xmtp-js/blob/main/content-types/content-type-reaction/src/Reaction.ts
impl ReactionCodec {
    const AUTHORITY_ID: &'static str = "xmtp.org";
    pub const TYPE_ID: &'static str = "reaction";
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

        Ok(EncodedContent {
            r#type: Some(ReactionCodec::content_type()),
            parameters: HashMap::new(),
            fallback: None,
            compression: None,
            content: buf,
        })
    }

    fn decode(content: EncodedContent) -> Result<ReactionV2, CodecError> {
        let decoded = ReactionV2::decode(content.content.as_slice())
            .map_err(|e| CodecError::Decode(e.to_string()))?;

        Ok(decoded)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use xmtp_proto::xmtp::mls::message_contents::content_types::{
        ReactionAction, ReactionSchema, ReactionV2,
    };

    use xmtp_common::rand_string;

    use super::*;

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
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
}
