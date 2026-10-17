//! Exact JSON representation shared by push senders and receivers.

use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};

/// A message location. Sequence identifiers remain decimal text in JSON.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PushPayload {
    pub topic: String,
    pub sequence_id: String,
}

impl PushPayload {
    /// Encode a topic and sequence identifier without loss of integer precision.
    // implements: PUSH-259
    pub fn new(topic: &[u8], sequence_id: u64) -> Self {
        Self {
            topic: STANDARD.encode(topic),
            sequence_id: sequence_id.to_string(),
        }
    }
}

/// Duration of an HMAC epoch in seconds.
pub const HMAC_EPOCH_SECONDS: i64 = 30 * 24 * 60 * 60;

/// Epoch used by sender HMAC keys, from Unix seconds.
pub fn hmac_epoch(unix_seconds: i64) -> i64 {
    unix_seconds / HMAC_EPOCH_SECONDS
}

/// Topic kinds accepted by push subscriptions.
// implements: PUSH-215
pub fn is_push_topic(kind: xmtp_proto::types::TopicKind) -> bool {
    matches!(
        kind,
        xmtp_proto::types::TopicKind::GroupMessagesV1
            | xmtp_proto::types::TopicKind::WelcomeMessagesV1
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // verifies: PUSH-259
    #[xmtp_common::test(unwrap_try = true)]
    fn json_preserves_sequence_precision() {
        let payload = PushPayload::new(&[1, 2, 3], u64::MAX);
        let json = serde_json::to_string(&payload)?;
        assert_eq!(
            json,
            r#"{"topic":"AQID","sequence_id":"18446744073709551615"}"#
        );
        assert_eq!(serde_json::from_str::<PushPayload>(&json)?, payload);
    }

    // verifies: PUSH-215
    #[xmtp_common::test(unwrap_try = true)]
    fn epoch_boundaries_and_topic_rules() {
        assert_eq!(hmac_epoch(HMAC_EPOCH_SECONDS - 1), 0);
        assert_eq!(hmac_epoch(HMAC_EPOCH_SECONDS), 1);
        assert_eq!(hmac_epoch(HMAC_EPOCH_SECONDS * 2), 2);
        use xmtp_proto::types::TopicKind;

        for kind in [TopicKind::GroupMessagesV1, TopicKind::WelcomeMessagesV1] {
            assert!(is_push_topic(kind));
        }
        for kind in [
            TopicKind::IdentityUpdatesV1,
            TopicKind::KeyPackagesV1,
            TopicKind::CommitLogEntriesV1,
        ] {
            assert!(!is_push_topic(kind));
        }
    }
}
