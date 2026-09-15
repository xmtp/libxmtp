use napi_derive::napi;
use xmtp_mls::client::notifications::{
  NotificationChannel as XmtpNotificationChannel, NotificationConfig as XmtpNotificationConfig,
  NotificationError, NotificationOverride as XmtpNotificationOverride,
  NotificationState as XmtpNotificationState,
};

#[napi(object)]
pub struct NotificationConfig {
  pub channel: String,
  pub token: Option<String>,
  pub url: Option<String>,
  pub signing_key: Option<Vec<u8>>,
  pub consent_states: Option<Vec<crate::consent_state::ConsentState>>,
  pub include_welcomes: Option<bool>,
  pub include_sync_groups: Option<bool>,
  pub include_commits: Option<bool>,
}

impl TryFrom<NotificationConfig> for XmtpNotificationConfig {
  type Error = NotificationError;

  fn try_from(value: NotificationConfig) -> Result<Self, Self::Error> {
    let channel = match (
      value.channel.as_str(),
      value.token,
      value.url,
      value.signing_key,
    ) {
      ("apns", Some(token), None, None) if !token.is_empty() => {
        XmtpNotificationChannel::Apns { token }
      }
      ("fcm", Some(token), None, None) if !token.is_empty() => {
        XmtpNotificationChannel::Fcm { token }
      }
      ("http", None, Some(url), Some(signing_key))
        if !url.is_empty() && (16..=64).contains(&signing_key.len()) =>
      {
        XmtpNotificationChannel::Http { url, signing_key }
      }
      _ => return Err(NotificationError::InvalidArgument),
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
    Ok(config)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[xmtp_common::test(unwrap_try = true)]
  fn notification_http_signing_key_boundaries() {
    for size in [15, 16, 64, 65] {
      let result = XmtpNotificationConfig::try_from(NotificationConfig {
        channel: "http".into(),
        token: None,
        url: Some("https://example.test".into()),
        signing_key: Some(vec![7; size]),
        consent_states: None,
        include_welcomes: None,
        include_sync_groups: None,
        include_commits: None,
      });
      if (16..=64).contains(&size) {
        assert!(
          matches!(result?.channel, XmtpNotificationChannel::Http { signing_key, .. } if signing_key.len() == size)
        );
      } else {
        assert!(matches!(result, Err(NotificationError::InvalidArgument)));
      }
    }
  }

  #[xmtp_common::test(unwrap_try = true)]
  fn notification_channel_discriminants_select_delivery() {
    let fcm: XmtpNotificationConfig = NotificationConfig {
      channel: "fcm".into(),
      token: Some("fcm-token".into()),
      url: None,
      signing_key: None,
      consent_states: None,
      include_welcomes: None,
      include_sync_groups: None,
      include_commits: None,
    }
    .try_into()?;

    assert!(matches!(
      fcm.channel,
      XmtpNotificationChannel::Fcm { token } if token == "fcm-token"
    ));

    let apns: XmtpNotificationConfig = NotificationConfig {
      channel: "apns".into(),
      token: Some("apns-token".into()),
      url: None,
      signing_key: None,
      consent_states: None,
      include_welcomes: None,
      include_sync_groups: None,
      include_commits: None,
    }
    .try_into()?;

    assert!(matches!(
      apns.channel,
      XmtpNotificationChannel::Apns { token } if token == "apns-token"
    ));
  }

  #[xmtp_common::test(unwrap_try = true)]
  fn notification_channel_rejects_invalid_combinations() {
    for config in [
      NotificationConfig {
        channel: "unknown".into(),
        token: Some("token".into()),
        url: None,
        signing_key: None,
        consent_states: None,
        include_welcomes: None,
        include_sync_groups: None,
        include_commits: None,
      },
      NotificationConfig {
        channel: "fcm".into(),
        token: None,
        url: None,
        signing_key: None,
        consent_states: None,
        include_welcomes: None,
        include_sync_groups: None,
        include_commits: None,
      },
      NotificationConfig {
        channel: "apns".into(),
        token: Some("token".into()),
        url: Some("https://example.test".into()),
        signing_key: None,
        consent_states: None,
        include_welcomes: None,
        include_sync_groups: None,
        include_commits: None,
      },
    ] {
      assert!(matches!(
        XmtpNotificationConfig::try_from(config),
        Err(NotificationError::InvalidArgument)
      ));
    }
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
