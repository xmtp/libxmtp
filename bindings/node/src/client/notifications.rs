use crate::{
  ErrorWrapper,
  client::Client,
  notifications::{NotificationConfig, NotificationState},
};
use napi::bindgen_prelude::Result;
use napi_derive::napi;

#[napi]
impl Client {
  #[napi]
  #[xmtp_common::err_span]
  pub async fn enable_notifications(
    &self,
    config: NotificationConfig,
  ) -> Result<NotificationState> {
    Ok(
      self
        .inner_client()
        .enable_notifications(config.try_into().map_err(ErrorWrapper::from)?)
        .await
        .map(Into::into)
        .map_err(ErrorWrapper::from)?,
    )
  }

  #[napi]
  #[xmtp_common::err_span]
  pub async fn disable_notifications(&self) -> Result<()> {
    self
      .inner_client()
      .disable_notifications()
      .await
      .map_err(ErrorWrapper::from)?;
    Ok(())
  }

  #[napi]
  #[xmtp_common::err_span]
  pub fn notification_state(&self) -> Result<NotificationState> {
    Ok(
      self
        .inner_client()
        .notification_state()
        .map(Into::into)
        .map_err(ErrorWrapper::from)?,
    )
  }
}
