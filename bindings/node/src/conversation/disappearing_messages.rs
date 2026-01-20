use crate::{ErrorWrapper, conversation::Conversation};
use napi::bindgen_prelude::{BigInt, Result};
use napi_derive::napi;
use xmtp_mls::mls_common::group_mutable_metadata::MessageDisappearingSettings as XmtpMessageDisappearingSettings;

#[napi(object)]
#[derive(Clone)]
pub struct MessageDisappearingSettings {
  pub from_ns: BigInt,
  pub in_ns: BigInt,
}

impl From<MessageDisappearingSettings> for XmtpMessageDisappearingSettings {
  fn from(value: MessageDisappearingSettings) -> Self {
    Self {
      from_ns: value.from_ns.get_i64().0,
      in_ns: value.in_ns.get_i64().0,
    }
  }
}

impl From<XmtpMessageDisappearingSettings> for MessageDisappearingSettings {
  fn from(value: XmtpMessageDisappearingSettings) -> Self {
    Self {
      from_ns: BigInt::from(value.from_ns),
      in_ns: BigInt::from(value.in_ns),
    }
  }
}

#[napi]
impl Conversation {
  #[napi]
  pub async fn update_message_disappearing_settings(
    &self,
    settings: MessageDisappearingSettings,
  ) -> Result<()> {
    let group = self.create_mls_group();
    group
      .update_conversation_message_disappearing_settings(settings.into())
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }

  #[napi]
  pub async fn remove_message_disappearing_settings(&self) -> Result<()> {
    let group = self.create_mls_group();

    group
      .remove_conversation_message_disappearing_settings()
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }

  #[napi]
  pub fn message_disappearing_settings(&self) -> Result<Option<MessageDisappearingSettings>> {
    let settings = self
      .create_mls_group()
      .disappearing_settings()
      .map_err(ErrorWrapper::from)?;

    match settings {
      Some(s) => Ok(Some(s.into())),
      None => Ok(None),
    }
  }

  #[napi]
  pub fn is_message_disappearing_enabled(&self) -> Result<bool> {
    self.message_disappearing_settings().map(|settings| {
      settings
        .as_ref()
        .is_some_and(|s| s.from_ns.get_i64().0 > 0 && s.in_ns.get_i64().0 > 0)
    })
  }
}
