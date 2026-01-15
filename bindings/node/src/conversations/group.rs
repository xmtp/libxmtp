use crate::ErrorWrapper;
use crate::conversation::Conversation;
use crate::conversation::disappearing_messages::MessageDisappearingSettings;
use crate::conversations::Conversations;
use crate::identity::Identifier;
use crate::permissions::{GroupPermissionsOptions, PermissionPolicySet};
use napi::bindgen_prelude::{Error, Result};
use napi_derive::napi;
use xmtp_mls::groups::PreconfiguredPolicies;
use xmtp_mls::mls_common::group::GroupMetadataOptions;

#[napi(object)]
#[derive(Clone)]
pub struct CreateGroupOptions {
  pub permissions: Option<GroupPermissionsOptions>,
  pub group_name: Option<String>,
  pub group_image_url_square: Option<String>,
  pub group_description: Option<String>,
  pub custom_permission_policy_set: Option<PermissionPolicySet>,
  pub message_disappearing_settings: Option<MessageDisappearingSettings>,
  pub app_data: Option<String>,
}

impl CreateGroupOptions {
  pub fn into_group_metadata_options(self) -> GroupMetadataOptions {
    GroupMetadataOptions {
      name: self.group_name,
      image_url_square: self.group_image_url_square,
      description: self.group_description,
      message_disappearing_settings: self
        .message_disappearing_settings
        .map(|settings| settings.into()),
      app_data: self.app_data,
    }
  }
}

#[napi]
impl Conversations {
  #[napi]
  pub fn create_group_optimistic(
    &self,
    options: Option<CreateGroupOptions>,
  ) -> Result<Conversation> {
    let options = options.unwrap_or(CreateGroupOptions {
      permissions: None,
      group_name: None,
      group_image_url_square: None,
      group_description: None,
      custom_permission_policy_set: None,
      message_disappearing_settings: None,
      app_data: None,
    });

    if let Some(GroupPermissionsOptions::CustomPolicy) = options.permissions {
      if options.custom_permission_policy_set.is_none() {
        return Err(Error::from_reason("CustomPolicy must include policy set"));
      }
    } else if options.custom_permission_policy_set.is_some() {
      return Err(Error::from_reason(
        "Only CustomPolicy may specify a policy set",
      ));
    }

    let metadata_options = options.clone().into_group_metadata_options();

    let group_permissions = match options.permissions {
      Some(GroupPermissionsOptions::Default) => {
        Some(PreconfiguredPolicies::Default.to_policy_set())
      }
      Some(GroupPermissionsOptions::AdminOnly) => {
        Some(PreconfiguredPolicies::AdminsOnly.to_policy_set())
      }
      Some(GroupPermissionsOptions::CustomPolicy) => {
        if let Some(policy_set) = options.custom_permission_policy_set {
          Some(policy_set.try_into().map_err(ErrorWrapper::from)?)
        } else {
          None
        }
      }
      _ => None,
    };

    let group = self
      .inner_client
      .create_group(group_permissions, Some(metadata_options))
      .map_err(ErrorWrapper::from)?;

    Ok(group.into())
  }

  #[napi]
  pub async fn create_group(
    &self,
    account_identities: Vec<Identifier>,
    options: Option<CreateGroupOptions>,
  ) -> Result<Conversation> {
    let convo = self.create_group_optimistic(options)?;

    if !account_identities.is_empty() {
      convo.add_members(account_identities).await?;
    } else {
      convo.sync().await.map_err(ErrorWrapper::from)?;
    };

    Ok(convo)
  }

  #[napi]
  pub async fn create_group_by_inbox_id(
    &self,
    inbox_ids: Vec<String>,
    options: Option<CreateGroupOptions>,
  ) -> Result<Conversation> {
    let convo = self.create_group_optimistic(options)?;

    if !inbox_ids.is_empty() {
      convo.add_members_by_inbox_id(inbox_ids).await?;
    } else {
      convo.sync().await.map_err(ErrorWrapper::from)?;
    }

    Ok(convo)
  }
}
