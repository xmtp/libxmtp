use std::collections::HashSet;

use crate::{CodecError, ContentCodec};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use xmtp_proto::xmtp::mls::message_contents::{ContentTypeId, EncodedContent};

const EXPIRY_RANGE_ERROR: &str = "Actions expiry is outside the signed 64-bit nanosecond range.";

const NANOS_PER_MILLISECOND: i64 = 1;

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

    // implements: CTYPE-019, CTYPE-030
    fn encode(actions: Actions) -> Result<EncodedContent, CodecError> {
        if !actions.expiry_is_representable()
            || actions.expiries().any(|expiry| {
                expiry
                    .timestamp_millis()
                    .checked_mul(NANOS_PER_MILLISECOND)
                    .is_none()
            })
        {
            return Err(CodecError::Encode(EXPIRY_RANGE_ERROR.into()));
        }
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

    // implements: CTYPE-030
    fn decode(actions: EncodedContent) -> Result<Actions, CodecError> {
        let actions: Actions = serde_json::from_slice(&actions.content)
            .map_err(|e| CodecError::Decode(format!("Unable to deserialize actions. {e:?}")))?;

        if !actions.expiry_is_representable() {
            return Err(CodecError::Decode(EXPIRY_RANGE_ERROR.into()));
        }
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

impl Actions {
    fn expiry_is_representable(&self) -> bool {
        self.expiries().all(|_expiry| true)
    }

    fn expiries(&self) -> impl Iterator<Item = &DateTime<Utc>> {
        self.expires_at.iter().chain(
            self.actions
                .iter()
                .filter_map(|action| action.expires_at.as_ref()),
        )
    }
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
    use chrono::{DateTime, TimeZone, Utc};
    use xmtp_proto::xmtp::mls::message_contents::EncodedContent;

    #[xmtp_common::test(unwrap_try = true)]
    // verifies: CTYPE-007, CTYPE-019
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
    // verifies: CTYPE-019
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

    // verifies: CTYPE-030
    #[xmtp_common::test(unwrap_try = true)]
    fn actions_expiry_checks_both_fields_and_range_limits() {
        let sample = || Actions {
            id: "choice".into(),
            description: "Choose".into(),
            actions: vec![Action {
                id: "one".into(),
                label: "One".into(),
                image_url: None,
                style: None,
                expires_at: None,
            }],
            expires_at: None,
        };
        // Encode uses milliseconds. These are the nearest valid whole milliseconds.
        for timestamp in ["1677-09-21T00:12:43.146Z", "2262-04-11T23:47:16.854Z"] {
            for nested in [false, true] {
                let mut actions = sample();
                let expiry = DateTime::parse_from_rfc3339(timestamp)?.with_timezone(&Utc);
                if nested {
                    actions.actions[0].expires_at = Some(expiry);
                } else {
                    actions.expires_at = Some(expiry);
                }
                assert_eq!(
                    ActionsCodec::decode(ActionsCodec::encode(actions.clone())?)?,
                    actions
                );
            }
        }
        for (timestamp, valid) in [
            ("1677-09-21T00:12:43.145224192Z", true),
            ("2262-04-11T23:47:16.854775807Z", true),
            ("1677-09-21T00:12:43.145224191Z", false),
            ("2262-04-11T23:47:16.854775808Z", false),
        ] {
            for nested in [false, true] {
                let mut actions = sample();
                let expiry = DateTime::parse_from_rfc3339(timestamp)?.with_timezone(&Utc);
                if nested {
                    actions.actions[0].expires_at = Some(expiry);
                } else {
                    actions.expires_at = Some(expiry);
                }
                let wire = EncodedContent {
                    r#type: Some(ActionsCodec::content_type()),
                    content: serde_json::to_vec(&actions)?,
                    ..Default::default()
                };
                // Keep nanosecond precision in received bytes, independent of the encoder.
                let mut json: serde_json::Value = serde_json::from_slice(&wire.content)?;
                if nested {
                    json["actions"][0]["expiresAt"] = timestamp.into();
                } else {
                    json["expiresAt"] = timestamp.into();
                }
                let result = ActionsCodec::decode(EncodedContent {
                    content: serde_json::to_vec(&json)?,
                    ..wire
                });
                if valid {
                    assert_eq!(result?, actions.clone());
                    if timestamp.starts_with("1677") {
                        // Millisecond truncation moves this expiry below the wire range.
                        assert!(matches!(
                            ActionsCodec::encode(actions),
                            Err(CodecError::Encode(_))
                        ));
                    } else {
                        assert!(ActionsCodec::decode(ActionsCodec::encode(actions)?).is_ok());
                    }
                } else {
                    assert!(matches!(result, Err(CodecError::Decode(_))), "{timestamp}");
                    assert!(
                        matches!(ActionsCodec::encode(actions), Err(CodecError::Encode(_))),
                        "{timestamp}"
                    );
                }
            }
        }
    }

    // verifies: CTYPE-030
    #[xmtp_common::test(unwrap_try = true)]
    fn last_action_expiry_is_checked() {
        for timestamp in [
            "1677-09-21T00:12:43.145224191Z",
            "2262-04-11T23:47:16.854775808Z",
        ] {
            let expiry = DateTime::parse_from_rfc3339(timestamp)?.with_timezone(&Utc);
            let actions = Actions {
                id: "choices".into(),
                description: "Choose".into(),
                actions: vec![
                    Action {
                        id: "one".into(),
                        label: "One".into(),
                        image_url: None,
                        style: None,
                        expires_at: None,
                    },
                    Action {
                        id: "two".into(),
                        label: "Two".into(),
                        image_url: None,
                        style: None,
                        expires_at: Some(expiry),
                    },
                ],
                expires_at: None,
            };
            assert!(matches!(
                ActionsCodec::encode(actions.clone()),
                Err(CodecError::Encode(_))
            ));
            let mut json = serde_json::to_value(actions)?;
            json["actions"][1]["expiresAt"] = timestamp.into();
            assert!(matches!(
                ActionsCodec::decode(EncodedContent {
                    r#type: Some(ActionsCodec::content_type()),
                    content: serde_json::to_vec(&json)?,
                    ..Default::default()
                }),
                Err(CodecError::Decode(_))
            ));
        }
    }
}
