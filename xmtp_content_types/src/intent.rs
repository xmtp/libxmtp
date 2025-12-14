use crate::{CodecError, ContentCodec};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use xmtp_proto::xmtp::mls::message_contents::{ContentTypeId, EncodedContent};

const INTENT_METADATA_LIMIT: usize = 10 * 1024;

pub struct IntentCodec;
impl IntentCodec {
    const AUTHORITY_ID: &str = "coinbase.com";
    pub const TYPE_ID: &str = "intent";
    pub const MAJOR_VERSION: u32 = 1;
    pub const MINOR_VERSION: u32 = 0;
}

impl IntentCodec {
    fn fallback(intent: &Intent) -> Option<String> {
        Some(format!("User selected action: {}", intent.action_id))
    }
}

impl ContentCodec<Intent> for IntentCodec {
    fn content_type() -> ContentTypeId {
        ContentTypeId {
            authority_id: Self::AUTHORITY_ID.to_string(),
            type_id: Self::TYPE_ID.to_string(),
            version_major: Self::MAJOR_VERSION,
            version_minor: Self::MINOR_VERSION,
        }
    }

    fn encode(intent: Intent) -> Result<EncodedContent, CodecError> {
        if let Some(metadata) = &intent.metadata {
            let intent_json = serde_json::to_vec(metadata).map_err(|e| {
                CodecError::Encode(format!("Unable to serialize intent metadata. {e:?}"))
            })?;
            if intent_json.len() > INTENT_METADATA_LIMIT {
                return Err(CodecError::Encode(format!(
                    "Intent metadata is too large. (limit: {}kb)",
                    INTENT_METADATA_LIMIT / 1024
                )));
            }
        }

        let intent_json = serde_json::to_vec(&intent)
            .map_err(|e| CodecError::Encode(format!("Unable to serialize intent. {e:?}")))?;

        Ok(EncodedContent {
            r#type: Some(Self::content_type()),
            content: intent_json,
            fallback: Self::fallback(&intent),
            ..Default::default()
        })
    }

    fn decode(intent: EncodedContent) -> Result<Intent, CodecError> {
        let intent = serde_json::from_slice(&intent.content)
            .map_err(|e| CodecError::Decode(format!("Unable to deserialize intent. {e:?}")))?;

        Ok(intent)
    }

    fn should_push() -> bool {
        true
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Intent {
    pub id: String,
    pub action_id: String,
    pub metadata: Option<HashMap<String, Value>>,
}

#[cfg(test)]
mod tests {
    use super::{Intent, IntentCodec};
    use crate::ContentCodec;
    use serde_json::Value;

    #[xmtp_common::test(unwrap_try = true)]
    fn encode_decode_intent() {
        let intent = Intent {
            id: "thanksgiving_selection".to_string(),
            action_id: "the_turkey_of_course".to_string(),
            metadata: Some(
                [
                    (
                        "amount_for_yourself".to_string(),
                        Value::String("7lbs".to_string()),
                    ),
                    ("hugs_from_grandma".to_string(), Value::Bool(true)),
                ]
                .into_iter()
                .collect(),
            ),
        };

        let encoded = IntentCodec::encode(intent.clone())?;
        assert_eq!(
            encoded.fallback(),
            "User selected action: the_turkey_of_course"
        );
        let decoded = IntentCodec::decode(encoded)?;

        assert_eq!(decoded, intent);
    }
}
