use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use xmtp_proto::xmtp::mls::message_contents::{ContentTypeId, EncodedContent};

use crate::{CodecError, ContentCodec};

pub struct ActionsCodec;
impl ActionsCodec {
    const AUTHORITY_ID: &str = "coinbase.com";
    pub const TYPE_ID: &str = "actions";
    pub const MAJOR_VERSION: u32 = 1;
    pub const MINOR_VERSION: u32 = 0;
}

impl ContentCodec<Actions> for ActionsCodec {
    fn content_type() -> ContentTypeId {
        ContentTypeId {
            authority_id: Self::AUTHORITY_ID.to_string(),
            type_id: Self::TYPE_ID.to_string(),
            version_major: Self::MAJOR_VERSION,
            version_minor: Self::MINOR_VERSION,
        }
    }

    fn encode(actions: Actions) -> Result<EncodedContent, CodecError> {
        if actions.actions.is_empty() {
            return Err(CodecError::Encode(
                "Actions must contain at least one action.".to_string(),
            ));
        }
        if actions.actions.len() > 10 {
            return Err(CodecError::Encode(
                "Actions cannot exceed 10 actions for UX reasons.".to_string(),
            ));
        }

        Ok(EncodedContent {
            r#type: Some(Self::content_type()),
            content: serde_json::to_vec(&actions)
                .map_err(|e| CodecError::Encode(format!("Unable to serialize actions. {e:?}")))?,
            ..Default::default()
        })
    }

    fn decode(content: EncodedContent) -> Result<Actions, CodecError> {
        let actions: Actions = serde_json::from_slice(&content.content)
            .map_err(|e| CodecError::Decode(format!("Unable to deserialize actions. {e:?}")))?;

        Ok(actions)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Actions {
    id: String,
    description: String,
    actions: Vec<Action>,
    expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Action {
    id: String,
    label: String,
    image_url: Option<String>,
    style: Option<ActionStyle>,
    expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum ActionStyle {
    Primary,
    Secondary,
    Danger,
}
