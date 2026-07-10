use crate::{ErrorWrapper, conversation::Conversation, conversations::ConversationType};
use napi::bindgen_prelude::Result;
use napi_derive::napi;
use xmtp_mls::mls_common::group_metadata::GroupMetadata as XmtpGroupMetadata;

/// Options for [`Conversation::enableProposals`]. Mirrors
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

#[napi]
pub struct GroupMetadata {
  metadata: XmtpGroupMetadata,
}

#[napi]
impl GroupMetadata {
  pub fn new(metadata: XmtpGroupMetadata) -> Self {
    Self { metadata }
  }

  #[napi]
  pub fn creator_inbox_id(&self) -> String {
    self.metadata.creator_inbox_id.clone()
  }

  #[napi]
  pub fn conversation_type(&self) -> ConversationType {
    self.metadata.conversation_type.into()
  }
}

#[napi]
impl Conversation {
  #[napi]
  #[xmtp_common::err_span]
  pub async fn group_metadata(&self) -> Result<GroupMetadata> {
    let group = self.create_mls_group();
    let metadata = group.metadata().await.map_err(ErrorWrapper::from)?;

    Ok(GroupMetadata::new(metadata))
  }

  #[napi]
  #[xmtp_common::err_span]
  pub fn group_name(&self) -> Result<String> {
    let group = self.create_mls_group();
    let group_name = group.group_name().map_err(ErrorWrapper::from)?;

    Ok(group_name)
  }

  #[napi]
  #[xmtp_common::err_span]
  pub async fn update_group_name(&self, group_name: String) -> Result<()> {
    let group = self.create_mls_group();

    group
      .update_group_name(group_name)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }

  /// Enable AppData-proposal-based metadata updates on this group.
  ///
  /// Stages the bootstrap commit that migrates the group's metadata
  /// from the legacy GroupContextExtensions shape into the OpenMLS
  /// AppData dictionary. Hard-fails if any member's latest key
  /// package doesn't advertise `ProposalType::AppDataUpdate`. One-
  /// way: migrated groups cannot return to the legacy path.
  #[napi]
  #[xmtp_common::err_span]
  pub async fn enable_proposals(&self, options: EnableProposalsOptions) -> Result<()> {
    let group = self.create_mls_group();
    group
      .enable_proposals(options.into())
      .await
      .map_err(|e| ErrorWrapper::from(e).into())
  }

  /// Whether this group has migrated to AppData-proposal-based
  /// metadata updates (the `AppDataDictionary` group-context
  /// extension is present). `false` means the group is still on
  /// the legacy GroupContextExtensions path.
  #[napi]
  #[xmtp_common::err_span]
  pub fn proposals_enabled(&self) -> Result<bool> {
    let group = self.create_mls_group();
    Ok(group.is_proposals_enabled().map_err(ErrorWrapper::from)?)
  }

  #[napi]
  #[xmtp_common::err_span]
  pub fn group_description(&self) -> Result<String> {
    let group = self.create_mls_group();
    let group_description = group.group_description().map_err(ErrorWrapper::from)?;

    Ok(group_description)
  }

  #[napi]
  #[xmtp_common::err_span]
  pub async fn update_group_description(&self, group_description: String) -> Result<()> {
    let group = self.create_mls_group();

    group
      .update_group_description(group_description)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }

  #[napi]
  #[xmtp_common::err_span]
  pub fn group_image_url_square(&self) -> Result<String> {
    let group = self.create_mls_group();

    let group_image_url_square = group.group_image_url_square().map_err(ErrorWrapper::from)?;

    Ok(group_image_url_square)
  }

  #[napi]
  #[xmtp_common::err_span]
  pub async fn update_group_image_url_square(&self, group_image_url_square: String) -> Result<()> {
    let group = self.create_mls_group();

    group
      .update_group_image_url_square(group_image_url_square)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }

  #[napi]
  #[xmtp_common::err_span]
  pub fn app_data(&self) -> Result<String> {
    let group = self.create_mls_group();
    let app_data = group.app_data().map_err(ErrorWrapper::from)?;

    Ok(app_data)
  }

  #[napi]
  #[xmtp_common::err_span]
  pub async fn update_app_data(&self, app_data: String) -> Result<()> {
    let group = self.create_mls_group();

    group
      .update_app_data(app_data)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }
}
