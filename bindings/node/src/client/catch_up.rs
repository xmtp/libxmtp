use crate::{ErrorWrapper, client::Client};
use napi::bindgen_prelude::Result;
use napi_derive::napi;

/// Counts of what a single `catchUpToLive` run brought into the local store.
#[napi(object)]
pub struct CatchUpSummary {
  /// Messages newly persisted during this run.
  pub messages: u32,
  /// Conversations newly joined during this run.
  pub conversations: u32,
  /// Whether the run reached the live edge. Always `true` here — the node
  /// binding has no caller deadline (see `catchUpToLive`).
  pub completed: bool,
}

impl From<xmtp_mls::subscriptions::catch_up::CatchUpSummary> for CatchUpSummary {
  fn from(summary: xmtp_mls::subscriptions::catch_up::CatchUpSummary) -> Self {
    // Saturate rather than let `as u32` wrap: a single catch-up run persisting
    // more than u32::MAX items is unreachable in practice, but clamping keeps a
    // pathological count honest and monotonic instead of wrapping it toward
    // zero.
    Self {
      messages: u32::try_from(summary.messages).unwrap_or(u32::MAX),
      conversations: u32::try_from(summary.conversations).unwrap_or(u32::MAX),
      completed: summary.completed,
    }
  }
}

#[napi]
impl Client {
  /// Bring the local store current with the network, then stop — a bounded,
  /// one-shot sync for agents that poll for new state rather than hold a live
  /// stream.
  ///
  /// Idempotent: everything processed is persisted and catch-up does not
  /// advance durable message cursors, so a later call resumes from durable
  /// state and one with nothing owed reports zero.
  ///
  /// There is no caller deadline (unlike the mobile FFI, which faces OS
  /// background budgets). The run is still internally bounded — capped retry
  /// attempts with backoff, a short post-finish drain, and the wire watchdog —
  /// so it cannot hang indefinitely on a healthy process. An agent that needs a
  /// hard wall-clock bound can race this against its own timer.
  #[napi]
  #[xmtp_common::err_span]
  pub async fn catch_up_to_live(&self) -> Result<CatchUpSummary> {
    let summary = self
      .inner_client
      .catch_up_to_live(None)
      .await
      .map_err(ErrorWrapper::from)?;
    Ok(summary.into())
  }
}
