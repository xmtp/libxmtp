//! Parse the `topic` and `sequence_id` fields of a push body.

use base64::{Engine, engine::general_purpose::STANDARD};
use xmtp_proto::types::TopicKind;

use crate::{ConversationId, InboxId, InstallationId, XmtpError};

/// The largest sequence id the backend assigns (API-286).
const MAX_SEQUENCE_ID: u64 = i64::MAX as u64;

/// A parsed topic (TOPIC-001), one variant per topic kind.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum Topic {
    /// `0x00`: group messages.
    GroupMessages { conversation_id: ConversationId },
    /// `0x01`: welcomes for an installation.
    WelcomeMessages { installation_id: InstallationId },
    /// `0x02`: identity updates for an inbox.
    IdentityUpdates { inbox_id: InboxId },
    /// `0x03`: key packages for an installation.
    KeyPackages { installation_id: InstallationId },
    /// `0x04`: commit-log entries. The commit-log id is lowercase hex. It is
    /// not a conversation ID (GMOD-041), so no conversation lookup takes it.
    CommitLogEntries { commit_log_id: String },
}

/// A validated push body (PUSH-259).
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct PushHint {
    pub topic: Topic,
    pub sequence_id: u64,
}

/// Validate the `topic` and `sequence_id` fields of a push body. `topic` is
/// standard base64 of a topic of any kind; the receiver decides which kinds
/// it handles. `sequence_id` is decimal digits from 1 through
/// 9223372036854775807. Any other input fails with `InvalidArgument`.
#[xmtp_macro::sdk_export(pure)]
pub fn parse_push_hint(topic: String, sequence_id: String) -> Result<PushHint, XmtpError> {
    Ok(PushHint {
        topic: parse_topic(&topic)?,
        sequence_id: parse_sequence_id(&sequence_id)?,
    })
}

fn parse_topic(text: &str) -> Result<Topic, XmtpError> {
    let invalid = |reason: String| XmtpError::invalid_argument(format!("push topic: {reason}"));
    let bytes = STANDARD
        .decode(text)
        .map_err(|_| invalid("not standard base64".into()))?;
    let topic =
        xmtp_proto::types::Topic::parse(&bytes).map_err(|error| invalid(error.to_string()))?;
    let identifier = hex::encode(topic.identifier());
    Ok(match topic.kind() {
        TopicKind::GroupMessagesV1 => Topic::GroupMessages {
            conversation_id: identifier.try_into()?,
        },
        TopicKind::WelcomeMessagesV1 => Topic::WelcomeMessages {
            installation_id: identifier.try_into()?,
        },
        TopicKind::IdentityUpdatesV1 => Topic::IdentityUpdates {
            inbox_id: identifier.try_into()?,
        },
        TopicKind::KeyPackagesV1 => Topic::KeyPackages {
            installation_id: identifier.try_into()?,
        },
        TopicKind::CommitLogEntriesV1 => Topic::CommitLogEntries {
            commit_log_id: identifier,
        },
        kind => return Err(invalid(format!("unsupported kind {kind}"))),
    })
}

fn parse_sequence_id(text: &str) -> Result<u64, XmtpError> {
    // `u64::from_str` alone also accepts a leading `+`.
    text.bytes()
        .all(|byte| byte.is_ascii_digit())
        .then(|| text.parse::<u64>().ok())
        .flatten()
        .filter(|value| (1..=MAX_SEQUENCE_ID).contains(value))
        .ok_or_else(|| {
            XmtpError::invalid_argument(format!(
                "push sequence id: not decimal digits from 1 through {MAX_SEQUENCE_ID}"
            ))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn topic(kind: u8, identifier: &[u8]) -> String {
        STANDARD.encode([&[kind], identifier].concat())
    }

    fn hint(topic: Topic, sequence_id: u64) -> Option<PushHint> {
        Some(PushHint { topic, sequence_id })
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn parse_push_hint_accepts_and_rejects() {
        let zeros = [0x00; 32];
        let ones = [0xff; 32];
        let id = |bytes: &[u8; 32]| hex::encode(bytes);
        let group = |bytes: &[u8; 32]| Topic::GroupMessages {
            conversation_id: id(bytes).try_into().unwrap(),
        };
        let welcome = |bytes: &[u8; 32]| Topic::WelcomeMessages {
            installation_id: id(bytes).try_into().unwrap(),
        };
        // The all-0xff group topic contains `/`, which the URL-safe alphabet spells `_`.
        let url_safe = topic(0x00, &ones).replace('/', "_");
        assert_ne!(url_safe, topic(0x00, &ones));

        let cases: Vec<(String, &str, Option<PushHint>)> = vec![
            (topic(0x00, &zeros), "1", hint(group(&zeros), 1)),
            (topic(0x00, &ones), "7", hint(group(&ones), 7)),
            (topic(0x01, &zeros), "1", hint(welcome(&zeros), 1)),
            (topic(0x01, &ones), "1", hint(welcome(&ones), 1)),
            (
                topic(0x02, &ones),
                "1",
                hint(
                    Topic::IdentityUpdates {
                        inbox_id: id(&ones).try_into()?,
                    },
                    1,
                ),
            ),
            (
                topic(0x03, &ones),
                "1",
                hint(
                    Topic::KeyPackages {
                        installation_id: id(&ones).try_into()?,
                    },
                    1,
                ),
            ),
            (
                topic(0x04, &ones),
                "1",
                hint(
                    Topic::CommitLogEntries {
                        commit_log_id: id(&ones),
                    },
                    1,
                ),
            ),
            // Base64 outside the standard alphabet, or with whitespace.
            (url_safe, "1", None),
            (format!("{}\n", topic(0x00, &ones)), "1", None),
            (format!(" {}", topic(0x00, &ones)), "1", None),
            // Lengths: empty, legacy 16-byte group id, short welcome, long group.
            (String::new(), "1", None),
            (topic(0x00, &[0xab; 16]), "1", None),
            (topic(0x01, &[0xab; 31]), "1", None),
            (topic(0x00, &[0xab; 33]), "1", None),
            // A kind outside TOPIC-001.
            (topic(0x05, &ones), "1", None),
            // Sequence ids.
            (topic(0x00, &ones), "", None),
            (topic(0x00, &ones), "+1", None),
            (topic(0x00, &ones), " 1", None),
            (topic(0x00, &ones), "1a", None),
            (topic(0x00, &ones), "0", None),
            (
                topic(0x00, &ones),
                "9223372036854775807",
                hint(group(&ones), 9223372036854775807),
            ),
            (topic(0x00, &ones), "9223372036854775808", None),
            (topic(0x00, &ones), "18446744073709551616", None),
        ];

        for (topic, sequence_id, expected) in cases {
            let result = parse_push_hint(topic.clone(), sequence_id.into());
            match expected {
                Some(expected) => assert_eq!(result?, expected, "{topic:?} {sequence_id:?}"),
                None => assert!(
                    matches!(result, Err(XmtpError::InvalidArgument(_))),
                    "{topic:?} {sequence_id:?} gave {result:?}"
                ),
            }
        }
    }
}
