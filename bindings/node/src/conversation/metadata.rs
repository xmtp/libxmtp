use crate::{ErrorWrapper, conversation::Conversation, conversations::ConversationType};
use napi::bindgen_prelude::Result;
use napi_derive::napi;
use xmtp_mls::mls_common::group_metadata::GroupMetadata as XmtpGroupMetadata;

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
  pub async fn group_name(&self) -> Result<String> {
    let group = self.create_mls_group();
    let group_name = group.group_name().await.map_err(ErrorWrapper::from)?;

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

  /// Whether this group has migrated to AppData-proposal-based
  /// metadata updates (the `AppDataDictionary` group-context
  /// extension is present). `false` means the group is still on
  /// the legacy GroupContextExtensions path.
  #[napi]
  #[xmtp_common::err_span]
  pub async fn proposals_enabled(&self) -> Result<bool> {
    let group = self.create_mls_group();
    Ok(
      group
        .is_proposals_enabled()
        .await
        .map_err(ErrorWrapper::from)?,
    )
  }

  #[napi]
  #[xmtp_common::err_span]
  pub async fn group_description(&self) -> Result<String> {
    let group = self.create_mls_group();
    let group_description = group
      .group_description()
      .await
      .map_err(ErrorWrapper::from)?;

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
  pub async fn group_image_url_square(&self) -> Result<String> {
    let group = self.create_mls_group();

    let group_image_url_square = group
      .group_image_url_square()
      .await
      .map_err(ErrorWrapper::from)?;

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
  pub async fn app_data(&self) -> Result<String> {
    let group = self.create_mls_group();
    let app_data = group.app_data().await.map_err(ErrorWrapper::from)?;

    Ok(app_data)
  }

  #[napi]
  #[xmtp_common::err_span]
  pub async fn update_app_data(&self, options: UpdateAppDataOptions) -> Result<()> {
    let group = self.create_mls_group();

    group
      .update_app_data(options.value, options.expected_value)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }
}

/// Options for [`Conversation::updateAppData`]. An object (rather than
/// a bare string parameter) so future knobs can be added without
/// breaking callers — same pattern as [`EnableProposalsOptions`].
/// New fields must be `Option` so the generated TS type stays non-breaking.
#[napi(object)]
#[derive(Clone, Default)]
pub struct UpdateAppDataOptions {
  /// The new value for the group's opaque `APP_DATA` string slot.
  pub value: String,
  /// Optional compare-and-swap guard. When set, the update is abandoned with
  /// an `AppDataSuperseded` error — rather than overwriting — if the committed
  /// value is no longer this, including when another member's commit wins the
  /// race after this update was published. Omit for the historical
  /// last-writer-wins behavior.
  pub expected_value: Option<String>,
}
