use crate::ErrorWrapper;
use crate::client::Client;
use napi_derive::napi;

#[napi(object)]
#[derive(Default)]
pub struct VisibilityConfirmationOptions {
  /// Overall timeout in milliseconds (default: 30000).
  pub timeout_ms: Option<u32>,
}

impl From<VisibilityConfirmationOptions> for xmtp_mls::client::VisibilityConfirmationOptions {
  fn from(opts: VisibilityConfirmationOptions) -> Self {
    let defaults = Self::default();
    Self {
      timeout_ms: opts
        .timeout_ms
        .map(|t| t as u64)
        .unwrap_or(defaults.timeout_ms),
    }
  }
}

#[napi]
impl Client {
  #[napi]
  #[xmtp_common::err_span]
  pub async fn wait_for_registration_visible(
    &self,
    options: Option<VisibilityConfirmationOptions>,
  ) -> napi::Result<()> {
    self
      .inner_client
      .wait_for_registration_visible(options.unwrap_or_default().into())
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }
}
