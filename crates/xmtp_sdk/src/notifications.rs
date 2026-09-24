use crate::{Client, ConsentState, XmtpError};
use xmtp_mls::client::notifications as core;

#[derive(Clone, Debug, uniffi::Enum)]
pub enum NotificationChannel {
    Apns { token: String },
    Fcm { token: String },
    Http { url: String, signing_key: Vec<u8> },
}

impl From<NotificationChannel> for core::NotificationChannel {
    fn from(value: NotificationChannel) -> Self {
        match value {
            NotificationChannel::Apns { token } => Self::Apns { token },
            NotificationChannel::Fcm { token } => Self::Fcm { token },
            NotificationChannel::Http { url, signing_key } => Self::Http { url, signing_key },
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct NotificationConfig {
    pub channel: NotificationChannel,
    #[uniffi(default = None)]
    pub consent_states: Option<Vec<ConsentState>>,
    #[uniffi(default = None)]
    pub include_welcomes: Option<bool>,
    #[uniffi(default = None)]
    pub include_sync_groups: Option<bool>,
    #[uniffi(default = None)]
    pub include_commits: Option<bool>,
}

impl From<NotificationConfig> for core::NotificationConfig {
    fn from(value: NotificationConfig) -> Self {
        let mut config = Self::new(value.channel.into());
        if let Some(states) = value.consent_states {
            config.consent_states = states.into_iter().map(Into::into).collect();
        }
        if let Some(enabled) = value.include_welcomes {
            config.include_welcomes = enabled;
        }
        if let Some(enabled) = value.include_sync_groups {
            config.include_sync_groups = enabled;
        }
        if let Some(enabled) = value.include_commits {
            config.include_commits = enabled;
        }
        config
    }
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum NotificationFailure {
    PermissionDenied,
    InvalidArgument,
    OutOfRange,
    Unimplemented,
    ChannelNotConfigured,
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum NotificationState {
    Disabled,
    Enabled,
    Failed { error: NotificationFailure },
}

impl From<core::NotificationState> for NotificationState {
    fn from(value: core::NotificationState) -> Self {
        match value {
            core::NotificationState::Disabled => Self::Disabled,
            core::NotificationState::Enabled => Self::Enabled,
            core::NotificationState::Failed(error) => Self::Failed {
                error: match error {
                    core::NotificationError::PermissionDenied => {
                        NotificationFailure::PermissionDenied
                    }
                    core::NotificationError::InvalidArgument => {
                        NotificationFailure::InvalidArgument
                    }
                    core::NotificationError::OutOfRange => NotificationFailure::OutOfRange,
                    core::NotificationError::Unimplemented => NotificationFailure::Unimplemented,
                    core::NotificationError::ChannelNotConfigured => {
                        NotificationFailure::ChannelNotConfigured
                    }
                    _ => NotificationFailure::Unimplemented,
                },
            },
        }
    }
}

#[xmtp_macro::sdk_export(native_only)]
impl Client {
    pub async fn enable_notifications(
        &self,
        config: NotificationConfig,
    ) -> Result<NotificationState, XmtpError> {
        self.ensure_open()?;
        self.inner
            .enable_notifications(config.into())
            .await
            .map(Into::into)
            .map_err(XmtpError::from_notification)
    }

    pub async fn disable_notifications(&self) -> Result<(), XmtpError> {
        self.ensure_open()?;
        self.inner
            .disable_notifications()
            .await
            .map_err(XmtpError::from_notification)
    }

    pub fn notification_state(&self) -> Result<NotificationState, XmtpError> {
        self.ensure_open()?;
        self.inner
            .notification_state()
            .map(Into::into)
            .map_err(XmtpError::unknown)
    }
}
