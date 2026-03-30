use crate::client::Client;
use crate::ErrorWrapper;
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
    use xmtp_mls::registration_visible::{Quorum, VisibilityConfirmationOptions as MlsOptions};

    let opts = options.unwrap_or_default();
    let quorum = match (opts.quorum_absolute, opts.quorum_percentage) {
      (Some(n), _) => Quorum::Absolute(n as usize),
      (_, Some(p)) => Quorum::Percentage(p as f32),
      _ => Quorum::Percentage(0.5),
    };
    let mls_opts = MlsOptions {
      quorum,
      timeout_ms: opts.timeout_ms.unwrap_or(30_000) as u64,
      sleep_interval_ms: opts.sleep_interval_ms.unwrap_or(500) as u64,
    };

    self
      .inner_client
      .wait_for_registration_visible(mls_opts)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }
}
