use super::{FfiConsentState, FfiConversation, FfiXmtpClient};
use crate::FfiError;
use xmtp_mls::client::notifications::{
    NotificationChannel, NotificationConfig, NotificationError, NotificationOverride,
    NotificationState,
};

/// The notification delivery endpoint.
#[derive(uniffi::Enum, Clone, Debug)]
pub enum FfiNotificationChannel {
    Apns { token: String },
    Fcm { token: String },
    Http { url: String, signing_key: Vec<u8> },
}

impl From<FfiNotificationChannel> for NotificationChannel {
    fn from(value: FfiNotificationChannel) -> Self {
        match value {
            FfiNotificationChannel::Apns { token } => Self::Apns { token },
            FfiNotificationChannel::Fcm { token } => Self::Fcm { token },
            FfiNotificationChannel::Http { url, signing_key } => Self::Http { url, signing_key },
        }
    }
}

/// Notification delivery details and topic-selection rules.
#[derive(uniffi::Record, Debug)]
pub struct FfiNotificationConfig {
    pub channel: FfiNotificationChannel,
    #[uniffi(default = None)]
    pub consent_states: Option<Vec<FfiConsentState>>,
    #[uniffi(default = None)]
    pub include_welcomes: Option<bool>,
    #[uniffi(default = None)]
    pub include_sync_groups: Option<bool>,
    #[uniffi(default = None)]
    pub include_commits: Option<bool>,
    #[uniffi(default = None)]
    pub metadata: Option<Vec<u8>>,
}

impl From<FfiNotificationConfig> for NotificationConfig {
    fn from(value: FfiNotificationConfig) -> Self {
        let mut config = Self::new(value.channel.into());
        if let Some(consent_states) = value.consent_states {
            config.consent_states = consent_states.into_iter().map(Into::into).collect();
        }
        if let Some(include_welcomes) = value.include_welcomes {
            config.include_welcomes = include_welcomes;
        }
        if let Some(include_sync_groups) = value.include_sync_groups {
            config.include_sync_groups = include_sync_groups;
        }
        if let Some(include_commits) = value.include_commits {
            config.include_commits = include_commits;
        }
        if let Some(metadata) = value.metadata {
            config.metadata = metadata;
        }
        config
    }
}

/// A terminal notification failure stored in the local notification state.
#[derive(uniffi::Enum, Clone, Debug)]
pub enum FfiNotificationFailure {
    PermissionDenied,
    InvalidArgument,
    OutOfRange,
    Unimplemented,
    ChannelNotConfigured,
}

impl From<NotificationError> for FfiNotificationFailure {
    fn from(value: NotificationError) -> Self {
        match value {
            NotificationError::PermissionDenied => Self::PermissionDenied,
            NotificationError::InvalidArgument => Self::InvalidArgument,
            NotificationError::OutOfRange => Self::OutOfRange,
            NotificationError::Unimplemented => Self::Unimplemented,
            NotificationError::ChannelNotConfigured => Self::ChannelNotConfigured,
            _ => unreachable!("only terminal notification errors are stored"),
        }
    }
}

/// The locally stored notification state.
#[derive(uniffi::Enum, Clone, Debug)]
pub enum FfiNotificationState {
    Disabled,
    Enabled,
    Failed { error: FfiNotificationFailure },
}

impl From<NotificationState> for FfiNotificationState {
    fn from(value: NotificationState) -> Self {
        match value {
            NotificationState::Disabled => Self::Disabled,
            NotificationState::Enabled => Self::Enabled,
            NotificationState::Failed(error) => Self::Failed {
                error: error.into(),
            },
        }
    }
}

/// A local notification override for one conversation.
#[derive(uniffi::Enum, Clone, Copy, Debug)]
pub enum FfiNotificationOverride {
    Enabled,
    Disabled,
    Default,
}

impl From<FfiNotificationOverride> for NotificationOverride {
    fn from(value: FfiNotificationOverride) -> Self {
        match value {
            FfiNotificationOverride::Enabled => Self::Enabled,
            FfiNotificationOverride::Disabled => Self::Disabled,
            FfiNotificationOverride::Default => Self::Default,
        }
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl FfiXmtpClient {
    #[xmtp_common::err_span]
    pub async fn enable_notifications(
        &self,
        config: FfiNotificationConfig,
    ) -> Result<FfiNotificationState, FfiError> {
        self.inner_client
            .enable_notifications(config.into())
            .await
            .map(Into::into)
            .map_err(Into::into)
    }

    #[xmtp_common::err_span]
    pub async fn disable_notifications(&self) -> Result<(), FfiError> {
        self.inner_client
            .disable_notifications()
            .await
            .map_err(Into::into)
    }

    #[xmtp_common::err_span]
    pub fn notification_state(&self) -> Result<FfiNotificationState, FfiError> {
        self.inner_client
            .notification_state()
            .map(Into::into)
            .map_err(Into::into)
    }
}

#[uniffi::export]
impl FfiConversation {
    #[xmtp_common::err_span]
    pub fn set_notifications(&self, value: FfiNotificationOverride) -> Result<(), FfiError> {
        self.inner
            .set_notifications(value.into())
            .map_err(Into::into)
    }

    #[xmtp_common::err_span]
    pub fn notifications_enabled(&self) -> Result<bool, FfiError> {
        self.inner.notifications_enabled().map_err(Into::into)
    }
}
