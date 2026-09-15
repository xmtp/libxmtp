use napi::bindgen_prelude::Either3;
use napi_derive::napi;
use xmtp_mls::client::notifications::{
  NotificationChannel as XmtpNotificationChannel, NotificationConfig as XmtpNotificationConfig,
  NotificationError, NotificationOverride as XmtpNotificationOverride,
  NotificationState as XmtpNotificationState,
};

#[napi(object)]
pub struct ApnsNotificationChannel {
  pub token: String,
}

#[napi(object)]
pub struct FcmNotificationChannel {
  pub token: String,
}

#[napi(object)]
pub struct HttpNotificationChannel {
  pub url: String,
  pub signing_key: Vec<u8>,
}

pub type NotificationChannel =
  Either3<ApnsNotificationChannel, FcmNotificationChannel, HttpNotificationChannel>;

#[napi(object)]
pub struct NotificationConfig {
  pub channel: NotificationChannel,
  pub consent_states: Option<Vec<crate::consent_state::ConsentState>>,
  pub include_welcomes: Option<bool>,
  pub include_sync_groups: Option<bool>,
  pub include_commits: Option<bool>,
  pub metadata: Option<Vec<u8>>,
}

impl From<NotificationConfig> for XmtpNotificationConfig {
  fn from(value: NotificationConfig) -> Self {
    let channel = match value.channel {
      Either3::A(channel) => XmtpNotificationChannel::Apns {
        token: channel.token,
      },
      Either3::B(channel) => XmtpNotificationChannel::Fcm {
        token: channel.token,
      },
      Either3::C(channel) => XmtpNotificationChannel::Http {
        url: channel.url,
        signing_key: channel.signing_key,
      },
    };
    let mut config = Self::new(channel);
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

#[napi]
pub enum NotificationFailure {
  PermissionDenied,
  InvalidArgument,
  OutOfRange,
  Unimplemented,
  ChannelNotConfigured,
}

impl From<NotificationError> for NotificationFailure {
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

#[napi]
pub enum NotificationStateKind {
  Disabled,
  Enabled,
  Failed,
}

#[napi(object)]
pub struct NotificationState {
  pub state: NotificationStateKind,
  pub failure: Option<NotificationFailure>,
}

impl From<XmtpNotificationState> for NotificationState {
  fn from(value: XmtpNotificationState) -> Self {
    match value {
      XmtpNotificationState::Disabled => Self {
        state: NotificationStateKind::Disabled,
        failure: None,
      },
      XmtpNotificationState::Enabled => Self {
        state: NotificationStateKind::Enabled,
        failure: None,
      },
      XmtpNotificationState::Failed(error) => Self {
        state: NotificationStateKind::Failed,
        failure: Some(error.into()),
      },
    }
  }
}

#[napi]
pub enum NotificationOverride {
  Enabled,
  Disabled,
  Default,
}

impl From<NotificationOverride> for XmtpNotificationOverride {
  fn from(value: NotificationOverride) -> Self {
    match value {
      NotificationOverride::Enabled => Self::Enabled,
      NotificationOverride::Disabled => Self::Disabled,
      NotificationOverride::Default => Self::Default,
    }
  }
}
