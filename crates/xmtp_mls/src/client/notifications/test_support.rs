//! Notification configuration and task identity for tests outside the
//! notification modules.

use crate::core::notifications::{NotificationChannel, NotificationConfig};
use xmtp_db::tasks::TaskDataHash;

/// An HTTP channel for tests that read only the peer's subscriptions.
pub fn config() -> NotificationConfig {
    NotificationConfig::new(NotificationChannel::Http {
        url: "https://127.0.0.1/push".into(),
        signing_key: vec![7; 32].into(),
    })
}

/// The data hash of the notification runner's task.
pub fn task_hash() -> TaskDataHash {
    crate::worker::notifications::task_hash()
}
