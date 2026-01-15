use crate::{
  ErrorWrapper,
  conversation::Conversation,
  permissions::{MetadataField, PermissionPolicy, PermissionUpdateType},
};
use napi::bindgen_prelude::Result;
use napi_derive::napi;
use xmtp_mls::{
  groups::intents::PermissionUpdateType as XmtpPermissionUpdateType,
  mls_common::group_mutable_metadata::MetadataField as XmtpMetadataField,
};

#[napi]
impl Conversation {
  #[napi]
  pub async fn update_permission_policy(
    &self,
    permission_update_type: PermissionUpdateType,
    permission_policy_option: PermissionPolicy,
    metadata_field: Option<MetadataField>,
  ) -> Result<()> {
    let group = self.create_mls_group();

    group
      .update_permission_policy(
        XmtpPermissionUpdateType::from(&permission_update_type),
        permission_policy_option
          .try_into()
          .map_err(ErrorWrapper::from)?,
        metadata_field.map(|field| XmtpMetadataField::from(&field)),
      )
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }
}
