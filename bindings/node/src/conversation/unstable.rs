use crate::{ErrorWrapper, conversation::Conversation};
use napi::bindgen_prelude::Result;
use napi_derive::napi;

/// Options for [`UnstableConversation::enableProposals`]. Mirrors
/// [`xmtp_mls::groups::EnableProposalsOptions`].
#[napi(object)]
pub struct EnableProposalsOptions {
  /// Skip the pre-flight key-package capability check. Post-d14n
  /// every client supports proposals by version floor alone; set
  /// `true` to bypass the per-member scan in that environment.
  pub force: Option<bool>,
  /// Override the `MIN_SUPPORTED_PROTOCOL_VERSION` floor. `None`
  /// defaults to `xmtp_configuration::PROPOSALS_MIN_PROTOCOL_VERSION`.
  pub min_version: Option<String>,
}

impl From<EnableProposalsOptions> for xmtp_mls::groups::EnableProposalsOptions {
  fn from(opts: EnableProposalsOptions) -> Self {
    xmtp_mls::groups::EnableProposalsOptions {
      force: opts.force.unwrap_or(false),
      min_version: opts.min_version,
    }
  }
}

/// The pre-release surface of a [`Conversation`], reached through
/// `conversation.unstable`.
///
/// Everything here is unstable: the API shape may still change and, in
/// some cases (see [`UnstableConversation::enable_proposals`]), the
/// effect is one-way and irreversible. Reaching into `.unstable` is the
/// deliberate opt-in. When an API graduates it moves onto
/// [`Conversation`] directly and is removed here, so callers of the
/// `unstable` form get a compile-time break to migrate against.
#[napi]
pub struct UnstableConversation {
  inner: Conversation,
}

#[napi]
impl UnstableConversation {
  pub fn new(inner: Conversation) -> Self {
    Self { inner }
  }

  /// Enable AppData-proposal-based metadata updates on this group.
  ///
  /// Stages the bootstrap commit that migrates the group's metadata
  /// from the legacy GroupContextExtensions shape into the OpenMLS
  /// AppData dictionary. Hard-fails if any member's latest key package
  /// doesn't advertise `ProposalType::AppDataUpdate`. One-way:
  /// migrated groups cannot return to the legacy path.
  #[napi]
  #[xmtp_common::err_span]
  pub async fn enable_proposals(&self, options: EnableProposalsOptions) -> Result<()> {
    let group = self.inner.create_mls_group();
    group
      .enable_proposals(options.into())
      .await
      .map_err(|e| ErrorWrapper::from(e).into())
  }
}

#[napi]
impl Conversation {
  /// Pre-release APIs, gated behind an explicit `.unstable` opt-in.
  /// See [`UnstableConversation`].
  #[napi(getter)]
  pub fn unstable(&self) -> UnstableConversation {
    UnstableConversation::new(self.clone())
  }
}
