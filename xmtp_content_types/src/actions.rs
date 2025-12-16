use std::collections::HashSet;

use crate::{CodecError, ContentCodec};
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use xmtp_proto::xmtp::mls::message_contents::{ContentTypeId, EncodedContent};

pub struct ActionsCodec;
impl ActionsCodec {
    const AUTHORITY_ID: &str = "coinbase.com";
    pub const TYPE_ID: &str = "actions";
    pub const MAJOR_VERSION: u32 = 1;
    pub const MINOR_VERSION: u32 = 0;
}

impl ActionsCodec {
    fn fallback(content: &Actions) -> Option<String> {
        let action_list = content
            .actions
            .iter()
            .enumerate()
            .map(|(i, a)| format!("[{}] {}", i + 1, a.label))
            .collect::<Vec<_>>()
            .join("\n");

        Some(format!(
            "{}\n\n{}\n\nReply with the number to select",
            content.description, action_list
        ))
    }
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

        if actions
            .actions
            .iter()
            .map(|a| &a.id)
            .collect::<HashSet<_>>()
            .len()
            != actions.actions.len()
        {
            return Err(CodecError::Encode("Action ids must be unique.".to_string()));
        }

        Ok(EncodedContent {
            r#type: Some(Self::content_type()),
            content: serde_json::to_vec(&actions)
                .map_err(|e| CodecError::Encode(format!("Unable to serialize actions. {e:?}")))?,
            fallback: Self::fallback(&actions),
            ..Default::default()
        })
    }

    fn decode(actions: EncodedContent) -> Result<Actions, CodecError> {
        let actions: Actions = serde_json::from_slice(&actions.content)
            .map_err(|e| CodecError::Decode(format!("Unable to deserialize actions. {e:?}")))?;

        Ok(actions)
    }

    fn should_push() -> bool {
        true
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Actions {
    pub id: String,
    pub description: String,
    pub actions: Vec<Action>,
    pub expires_at: Option<NaiveDateTime>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Action {
    pub id: String,
    pub label: String,
    pub image_url: Option<String>,
    pub style: Option<ActionStyle>,
    pub expires_at: Option<NaiveDateTime>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ActionStyle {
    Primary,
    Secondary,
    Danger,
}

#[cfg(test)]
mod tests {
    use super::{Action, ActionStyle, Actions, ActionsCodec};
    use crate::{CodecError, ContentCodec};
    use chrono::NaiveDateTime;

    #[xmtp_common::test(unwrap_try = true)]
    fn encode_decode_actions() {
        let mut actions = Actions {
            id: "thanksgiving_selection".to_string(),
            description: "Grandma is asking for your input on Thanksgiving".to_string(),
            actions: vec![
                Action {
                    id: "the_turkey_of_course".to_string(),
                    label: "The Turkey (of course)".to_string(),
                    image_url: Some("http://turkey-images.biz/the-one.jpg".to_string()),
                    style: Some(ActionStyle::Primary),
                    expires_at: None,
                },
                Action {
                    id: "pork_loin".to_string(),
                    label: "Pork Loin".to_string(),
                    image_url: None,
                    style: None,
                    expires_at: Some(NaiveDateTime::MIN),
                },
            ],
            expires_at: Some(NaiveDateTime::MAX),
        };

        let encoded = ActionsCodec::encode(actions.clone())?;
        assert!(
            encoded
                .fallback()
                .contains("[1] The Turkey (of course)\n[2] Pork Loin"),
        );
        let decoded = ActionsCodec::decode(encoded)?;

        assert_eq!(decoded, actions);

        actions.actions.push(Action {
            id: "pork_loin".to_string(),
            label: "More Pork Loin".to_string(),
            image_url: None,
            style: None,
            expires_at: None,
        });

        let encoded_result = ActionsCodec::encode(actions);
        let Err(CodecError::Encode(reason)) = encoded_result else {
            panic!("Expected an uniqueness encoding error.");
        };
        assert!(reason.contains("unique"));
    }
}
