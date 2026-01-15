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
  pub async fn group_metadata(&self) -> Result<GroupMetadata> {
    let group = self.create_mls_group();
    let metadata = group.metadata().await.map_err(ErrorWrapper::from)?;

    Ok(GroupMetadata::new(metadata))
  }

  #[napi]
  pub fn group_name(&self) -> Result<String> {
    let group = self.create_mls_group();
    let group_name = group.group_name().map_err(ErrorWrapper::from)?;

    Ok(group_name)
  }

  #[napi]
  pub async fn update_group_name(&self, group_name: String) -> Result<()> {
    let group = self.create_mls_group();

    group
      .update_group_name(group_name)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }

  #[napi]
  pub fn group_description(&self) -> Result<String> {
    let group = self.create_mls_group();
    let group_description = group.group_description().map_err(ErrorWrapper::from)?;

    Ok(group_description)
  }

  #[napi]
  pub async fn update_group_description(&self, group_description: String) -> Result<()> {
    let group = self.create_mls_group();

    group
      .update_group_description(group_description)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }

  #[napi]
  pub fn group_image_url_square(&self) -> Result<String> {
    let group = self.create_mls_group();

    let group_image_url_square = group.group_image_url_square().map_err(ErrorWrapper::from)?;

    Ok(group_image_url_square)
  }

  #[napi]
  pub async fn update_group_image_url_square(&self, group_image_url_square: String) -> Result<()> {
    let group = self.create_mls_group();

    group
      .update_group_image_url_square(group_image_url_square)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }

  #[napi]
  pub fn app_data(&self) -> Result<String> {
    let group = self.create_mls_group();
    let app_data = group.app_data().map_err(ErrorWrapper::from)?;

    Ok(app_data)
  }

  #[napi]
  pub async fn update_app_data(&self, app_data: String) -> Result<()> {
    let group = self.create_mls_group();

    group
      .update_app_data(app_data)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }
}
