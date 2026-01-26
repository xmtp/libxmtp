use std::collections::HashSet;

use crate::{CodecError, ContentCodec};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use xmtp_proto::xmtp::mls::message_contents::{ContentTypeId, EncodedContent};

const UTC_MILLIS_FORMAT: &str = "%Y-%m-%dT%H:%M:%S%.3fZ";

mod datetime_utc_millis_option {
    use super::*;

    pub fn serialize<S>(date: &Option<DateTime<Utc>>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match date {
            Some(dt) => serializer.serialize_some(&dt.format(UTC_MILLIS_FORMAT).to_string()),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<DateTime<Utc>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let opt: Option<String> = Option::deserialize(deserializer)?;
        match opt {
            Some(s) => DateTime::parse_from_rfc3339(&s)
                .map(|dt| Some(dt.with_timezone(&Utc)))
                .map_err(serde::de::Error::custom),
            None => Ok(None),
        }
    }
}

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
    #[serde(
        default,
        alias = "expires_at",
        rename = "expiresAt",
        skip_serializing_if = "Option::is_none",
        with = "datetime_utc_millis_option"
    )]
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Action {
    pub id: String,
    pub label: String,
    #[serde(
        alias = "image_url",
        rename = "imageUrl",
        skip_serializing_if = "Option::is_none"
    )]
    pub image_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<ActionStyle>,
    #[serde(
        default,
        alias = "expires_at",
        rename = "expiresAt",
        skip_serializing_if = "Option::is_none",
        with = "datetime_utc_millis_option"
    )]
    pub expires_at: Option<DateTime<Utc>>,
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
    use chrono::{TimeZone, Utc};

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
                    expires_at: Some(Utc.with_ymd_and_hms(2025, 1, 15, 12, 30, 45).unwrap()),
                },
            ],
            expires_at: Some(Utc.with_ymd_and_hms(2025, 12, 31, 23, 59, 59).unwrap()),
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

    #[xmtp_common::test(unwrap_try = true)]
    fn expires_at_serializes_as_utc_with_millis() {
        let actions = Actions {
            id: "test".to_string(),
            description: "Test".to_string(),
            actions: vec![Action {
                id: "action1".to_string(),
                label: "Action 1".to_string(),
                image_url: None,
                style: None,
                expires_at: Some(Utc.with_ymd_and_hms(2025, 6, 15, 10, 30, 45).unwrap()),
            }],
            expires_at: Some(Utc.with_ymd_and_hms(2025, 12, 25, 23, 59, 59).unwrap()),
        };

        let encoded = ActionsCodec::encode(actions)?;
        let json = String::from_utf8(encoded.content.clone())?;

        // Verify format includes milliseconds (.000) and UTC suffix (Z)
        assert!(
            json.contains("2025-06-15T10:30:45.000Z"),
            "Action expires_at should be formatted with milliseconds and UTC: {json}"
        );
        assert!(
            json.contains("2025-12-25T23:59:59.000Z"),
            "Actions expires_at should be formatted with milliseconds and UTC: {json}"
        );
    }
}
