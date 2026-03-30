use crate::ErrorWrapper;
use crate::client::Client;
use napi_derive::napi;

#[napi(object)]
#[derive(Default)]
pub struct VisibilityConfirmationOptions {
  /// Fraction of nodes (0.0–1.0) that must confirm. Used if quorum_absolute is not set.
  pub quorum_percentage: Option<f64>,
  /// Exact number of nodes that must confirm. Takes precedence over quorum_percentage.
  pub quorum_absolute: Option<u32>,
  /// Overall timeout in milliseconds (default: 30000).
  pub timeout_ms: Option<u32>,
  /// Sleep interval between retries in milliseconds (default: 500).
  pub sleep_interval_ms: Option<u32>,
}

#[napi]
impl Client {
  #[napi]
  pub async fn wait_for_registration_visible(
    &self,
    options: Option<VisibilityConfirmationOptions>,
  ) -> napi::Result<()> {
    use xmtp_mls::registration_visible::VisibilityConfirmationOptions as MlsOptions;

    let opts = options.unwrap_or_default();
    let mls_opts = MlsOptions::from_parts(
      opts.quorum_percentage.map(|p| p as f32),
      opts.quorum_absolute.map(|n| n as usize),
      opts.timeout_ms.map(|t| t as u64),
      opts.sleep_interval_ms.map(|s| s as u64),
    );

    self
      .inner_client
      .wait_for_registration_visible(mls_opts)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }
}
