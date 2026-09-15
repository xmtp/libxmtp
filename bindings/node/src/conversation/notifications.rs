use crate::{ErrorWrapper, conversation::Conversation, notifications::NotificationOverride};
use napi::bindgen_prelude::Result;
use napi_derive::napi;

#[napi]
impl Conversation {
  #[napi]
  #[xmtp_common::err_span]
  pub fn set_notifications(&self, value: NotificationOverride) -> Result<()> {
    self
      .create_mls_group()
      .set_notifications(value.into())
      .map_err(ErrorWrapper::from)?;
    Ok(())
  }

  #[napi]
  #[xmtp_common::err_span]
  pub fn notifications_enabled(&self) -> Result<bool> {
    Ok(
      self
        .create_mls_group()
        .notifications_enabled()
        .map_err(ErrorWrapper::from)?,
    )
  }
}
