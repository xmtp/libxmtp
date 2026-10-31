//! The local notification settings model and the pure rules over it.

use crate::context::XmtpSharedContext;
use serde::{Deserialize, Serialize};
use xmtp_cryptography::secret::{Secret, Zeroizing};
use xmtp_db::consent_record::ConsentState;
use xmtp_db::{StorageError, StoredSecret, notifications::StoredNotification};
use xmtp_proto::backend_v1::{self, register_request::Delivery};

pub(crate) fn notification_settings_changed(context: &impl XmtpSharedContext) {
    context.task_channels().mark_notification_changed();
    xmtp_events::EventWriter::emit(
        context.events(),
        None,
        Some(crate::core::events::InternalEvent::NotificationSettingsChanged),
    );
}

/// The delivery endpoint. Credentials are omitted from debug output.
///
/// The HTTP signing key serializes as a `Vec<u8>` does, so stored
/// configurations do not change.
#[derive(Clone, Serialize, Deserialize)]
pub enum NotificationChannel {
    Apns { token: String },
    Fcm { token: String },
    Http { url: String, signing_key: Secret },
}

impl std::fmt::Debug for NotificationChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Apns { .. } => "Apns",
            Self::Fcm { .. } => "Fcm",
            Self::Http { .. } => "Http",
        })
    }
}

/// Delivery details and rules used to compute the desired subscriptions.
#[derive(Clone, Serialize, Deserialize)]
pub struct NotificationConfig {
    pub channel: NotificationChannel,
    pub consent_states: Vec<ConsentState>,
    pub include_welcomes: bool,
    pub include_sync_groups: bool,
    pub include_commits: bool,
}

impl NotificationConfig {
    /// Use the standard rules with the supplied delivery endpoint.
    pub fn new(channel: NotificationChannel) -> Self {
        Self {
            channel,
            consent_states: vec![ConsentState::Allowed],
            include_welcomes: true,
            include_sync_groups: false,
            include_commits: false,
        }
    }

    pub(crate) fn channel_id(&self) -> i32 {
        match self.channel {
            NotificationChannel::Apns { .. } => backend_v1::Channel::Apns as i32,
            NotificationChannel::Fcm { .. } => backend_v1::Channel::Fcm as i32,
            NotificationChannel::Http { .. } => backend_v1::Channel::Http as i32,
        }
    }

    /// The request copies the signing key and the recipient secret. Each copy
    /// of the request wipes them on drop.
    pub(crate) fn registration(&self, record: &StoredNotification) -> backend_v1::RegisterRequest {
        let delivery = match &self.channel {
            NotificationChannel::Apns { token } => Delivery::Apns(backend_v1::ApnsDelivery {
                token: token.clone(),
            }),
            NotificationChannel::Fcm { token } => Delivery::Fcm(backend_v1::FcmDelivery {
                token: token.clone(),
            }),
            NotificationChannel::Http { url, signing_key } => {
                Delivery::Http(backend_v1::HttpDelivery {
                    url: url.clone(),
                    signing_key: signing_key.as_slice().to_vec(),
                })
            }
        };
        backend_v1::RegisterRequest {
            recipient_id: record.push_recipient_id.clone().unwrap_or_default(),
            recipient_secret: recipient_secret(record),
            delivery: Some(delivery),
        }
    }
}

/// A copy of the stored recipient secret for a request, which wipes it on drop.
pub(crate) fn recipient_secret(record: &StoredNotification) -> Vec<u8> {
    record
        .push_recipient_secret
        .as_ref()
        .map(|secret| secret.as_slice().to_vec())
        .unwrap_or_default()
}

/// A conversation rule. Sync groups use the client rule only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotificationOverride {
    Enabled,
    Disabled,
    Default,
}

pub(crate) fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, StorageError> {
    serde_json::to_vec(value).map_err(|_| StorageError::DbSerialize)
}

pub(crate) fn decode<T: serde::de::DeserializeOwned>(value: &[u8]) -> Result<T, StorageError> {
    serde_json::from_slice(value).map_err(|_| StorageError::DbDeserialize)
}

/// Encode a configuration, which holds the HTTP signing key, for storage.
///
/// The same JSON as [`encode`]. A first pass measures it, so the zeroizing
/// buffer never grows and frees a partial copy of the key.
pub(crate) fn encode_config(config: &NotificationConfig) -> Result<StoredSecret, StorageError> {
    struct Measure(usize);
    impl std::io::Write for Measure {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 += bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut measure = Measure(0);
    serde_json::to_writer(&mut measure, config).map_err(|_| StorageError::DbSerialize)?;
    let mut bytes = Zeroizing::new(Vec::with_capacity(measure.0));
    serde_json::to_writer(&mut *bytes, config).map_err(|_| StorageError::DbSerialize)?;
    Ok(bytes.into())
}

/// Decode the stored configuration. The stored bytes stay in their
/// `StoredSecret`, and the signing key decodes straight into a `Secret`.
pub(crate) fn stored_config(
    record: &StoredNotification,
) -> Result<NotificationConfig, StorageError> {
    decode(
        record
            .push_config
            .as_ref()
            .ok_or(StorageError::DbDeserialize)?
            .as_slice(),
    )
}

/// Resolve only local rules. Active membership is checked separately by the task.
pub(crate) fn effective(
    config: &NotificationConfig,
    group: &xmtp_db::group::StoredGroup,
    consent: ConsentState,
) -> bool {
    use xmtp_proto::types::ConversationType;
    if group.conversation_type == ConversationType::Sync {
        return config.include_sync_groups;
    }
    if !matches!(
        group.conversation_type,
        ConversationType::Group | ConversationType::Dm
    ) {
        return false;
    }
    match group.push_override {
        Some(1) => true,
        Some(0) => false,
        _ => config.consent_states.contains(&consent),
    }
}

pub(crate) fn enabled_from_record(
    record: &StoredNotification,
    group: &xmtp_db::group::StoredGroup,
    consent: ConsentState,
) -> Result<bool, StorageError> {
    if record.push_state != 1 {
        return Ok(false);
    }
    let config = stored_config(record)?;
    Ok(effective(&config, group, consent))
}
