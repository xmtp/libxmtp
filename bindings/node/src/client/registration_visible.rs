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
}

impl From<VisibilityConfirmationOptions>
  for xmtp_mls::registration_visible::VisibilityConfirmationOptions
{
  fn from(opts: VisibilityConfirmationOptions) -> Self {
    use xmtp_mls::registration_visible::Quorum;

    let defaults = Self::default();
    let quorum = match (opts.quorum_absolute, opts.quorum_percentage) {
      (Some(n), _) => Quorum::Absolute(n as usize),
      (_, Some(p)) => Quorum::percentage(p as f32),
      _ => defaults.quorum,
    };
    Self {
      quorum,
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
