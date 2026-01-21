use napi::bindgen_prelude::Result;
use napi_derive::napi;
use std::collections::HashMap;
use xmtp_mls::{
  groups::{
    PreconfiguredPolicies,
    group_permissions::{
      BasePolicies, GroupMutablePermissions, GroupMutablePermissionsError, MembershipPolicies,
      MetadataBasePolicies, MetadataPolicies, PermissionsBasePolicies, PermissionsPolicies,
      PolicySet,
    },
    intents::{PermissionPolicyOption, PermissionUpdateType as XmtpPermissionUpdateType},
  },
  mls_common::group_mutable_metadata::MetadataField as XmtpMetadataField,
};

#[napi]
#[derive(Clone)]
pub enum GroupPermissionsOptions {
  Default,
  AdminOnly,
  CustomPolicy,
}

#[napi]
pub enum PermissionUpdateType {
  AddMember,
  RemoveMember,
  AddAdmin,
  RemoveAdmin,
  UpdateMetadata,
}

impl From<&PermissionUpdateType> for XmtpPermissionUpdateType {
  fn from(update_type: &PermissionUpdateType) -> Self {
    match update_type {
      PermissionUpdateType::AddMember => XmtpPermissionUpdateType::AddMember,
      PermissionUpdateType::RemoveMember => XmtpPermissionUpdateType::RemoveMember,
      PermissionUpdateType::AddAdmin => XmtpPermissionUpdateType::AddAdmin,
      PermissionUpdateType::RemoveAdmin => XmtpPermissionUpdateType::RemoveAdmin,
      PermissionUpdateType::UpdateMetadata => XmtpPermissionUpdateType::UpdateMetadata,
    }
  }
}

#[napi]
#[derive(Clone)]
pub enum PermissionPolicy {
  Allow,
  Deny,
  Admin,
  SuperAdmin,
  DoesNotExist,
  Other,
}

impl TryInto<PermissionPolicyOption> for PermissionPolicy {
  type Error = GroupMutablePermissionsError;

  fn try_into(self) -> std::result::Result<PermissionPolicyOption, Self::Error> {
    match self {
      PermissionPolicy::Allow => Ok(PermissionPolicyOption::Allow),
      PermissionPolicy::Deny => Ok(PermissionPolicyOption::Deny),
      PermissionPolicy::Admin => Ok(PermissionPolicyOption::AdminOnly),
      PermissionPolicy::SuperAdmin => Ok(PermissionPolicyOption::SuperAdminOnly),
      _ => Err(GroupMutablePermissionsError::InvalidPermissionPolicyOption),
    }
  }
}

impl From<&MembershipPolicies> for PermissionPolicy {
  fn from(policies: &MembershipPolicies) -> Self {
    if let MembershipPolicies::Standard(base_policy) = policies {
      match base_policy {
        BasePolicies::Allow => PermissionPolicy::Allow,
        BasePolicies::Deny => PermissionPolicy::Deny,
        BasePolicies::AllowSameMember => PermissionPolicy::Other,
        BasePolicies::AllowIfAdminOrSuperAdmin => PermissionPolicy::Admin,
        BasePolicies::AllowIfSuperAdmin => PermissionPolicy::SuperAdmin,
      }
    } else {
      PermissionPolicy::Other
    }
  }
}

impl From<&MetadataPolicies> for PermissionPolicy {
  fn from(policies: &MetadataPolicies) -> Self {
    if let MetadataPolicies::Standard(base_policy) = policies {
      match base_policy {
        MetadataBasePolicies::Allow => PermissionPolicy::Allow,
        MetadataBasePolicies::Deny => PermissionPolicy::Deny,
        MetadataBasePolicies::AllowIfActorAdminOrSuperAdmin => PermissionPolicy::Admin,
        MetadataBasePolicies::AllowIfActorSuperAdmin => PermissionPolicy::SuperAdmin,
      }
    } else {
      PermissionPolicy::Other
    }
  }
}

impl From<&PermissionsPolicies> for PermissionPolicy {
  fn from(policies: &PermissionsPolicies) -> Self {
    if let PermissionsPolicies::Standard(base_policy) = policies {
      match base_policy {
        PermissionsBasePolicies::Deny => PermissionPolicy::Deny,
        PermissionsBasePolicies::AllowIfActorAdminOrSuperAdmin => PermissionPolicy::Admin,
        PermissionsBasePolicies::AllowIfActorSuperAdmin => PermissionPolicy::SuperAdmin,
      }
    } else {
      PermissionPolicy::Other
    }
  }
}

impl TryInto<MetadataPolicies> for PermissionPolicy {
  type Error = GroupMutablePermissionsError;

  fn try_into(self) -> std::result::Result<MetadataPolicies, GroupMutablePermissionsError> {
    match self {
      PermissionPolicy::Allow => Ok(MetadataPolicies::allow()),
      PermissionPolicy::Deny => Ok(MetadataPolicies::deny()),
      PermissionPolicy::Admin => Ok(MetadataPolicies::allow_if_actor_admin()),
      PermissionPolicy::SuperAdmin => Ok(MetadataPolicies::allow_if_actor_super_admin()),
      _ => Err(GroupMutablePermissionsError::InvalidPermissionPolicyOption),
    }
  }
}

impl TryInto<PermissionsPolicies> for PermissionPolicy {
  type Error = GroupMutablePermissionsError;

  fn try_into(self) -> std::result::Result<PermissionsPolicies, Self::Error> {
    match self {
      PermissionPolicy::Deny => Ok(PermissionsPolicies::deny()),
      PermissionPolicy::Admin => Ok(PermissionsPolicies::allow_if_actor_admin()),
      PermissionPolicy::SuperAdmin => Ok(PermissionsPolicies::allow_if_actor_super_admin()),
      _ => Err(GroupMutablePermissionsError::InvalidPermissionPolicyOption),
    }
  }
}

impl TryInto<MembershipPolicies> for PermissionPolicy {
  type Error = GroupMutablePermissionsError;

  fn try_into(self) -> std::result::Result<MembershipPolicies, Self::Error> {
    match self {
      PermissionPolicy::Allow => Ok(MembershipPolicies::allow()),
      PermissionPolicy::Deny => Ok(MembershipPolicies::deny()),
      PermissionPolicy::Admin => Ok(MembershipPolicies::allow_if_actor_admin()),
      PermissionPolicy::SuperAdmin => Ok(MembershipPolicies::allow_if_actor_super_admin()),
      _ => Err(GroupMutablePermissionsError::InvalidPermissionPolicyOption),
    }
  }
}

#[napi(object)]
#[derive(Clone)]
pub struct PermissionPolicySet {
  pub add_member_policy: PermissionPolicy,
  pub remove_member_policy: PermissionPolicy,
  pub add_admin_policy: PermissionPolicy,
  pub remove_admin_policy: PermissionPolicy,
  pub update_group_name_policy: PermissionPolicy,
  pub update_group_description_policy: PermissionPolicy,
  pub update_group_image_url_square_policy: PermissionPolicy,
  pub update_message_disappearing_policy: PermissionPolicy,
  pub update_app_data_policy: PermissionPolicy,
}

impl From<PreconfiguredPolicies> for GroupPermissionsOptions {
  fn from(policy: PreconfiguredPolicies) -> Self {
    match policy {
      PreconfiguredPolicies::Default => GroupPermissionsOptions::Default,
      PreconfiguredPolicies::AdminsOnly => GroupPermissionsOptions::AdminOnly,
    }
  }
}

#[napi]
pub struct GroupPermissions {
  inner: GroupMutablePermissions,
}

#[napi]
impl GroupPermissions {
  pub fn new(permissions: GroupMutablePermissions) -> Self {
    Self { inner: permissions }
  }

  #[napi]
  pub fn policy_type(&self) -> Result<GroupPermissionsOptions> {
    if let Ok(preconfigured_policy) = self.inner.preconfigured_policy() {
      Ok(preconfigured_policy.into())
    } else {
      Ok(GroupPermissionsOptions::CustomPolicy)
    }
  }

  #[napi]
  pub fn policy_set(&self) -> Result<PermissionPolicySet> {
    let policy_set = &self.inner.policies;
    let metadata_policy_map = &policy_set.update_metadata_policy;
    let get_policy = |field: &str| {
      metadata_policy_map
        .get(field)
        .map(PermissionPolicy::from)
        .unwrap_or(PermissionPolicy::DoesNotExist)
    };
    Ok(PermissionPolicySet {
      add_member_policy: PermissionPolicy::from(&policy_set.add_member_policy),
      remove_member_policy: PermissionPolicy::from(&policy_set.remove_member_policy),
      add_admin_policy: PermissionPolicy::from(&policy_set.add_admin_policy),
      remove_admin_policy: PermissionPolicy::from(&policy_set.remove_admin_policy),
      update_group_name_policy: get_policy(XmtpMetadataField::GroupName.as_str()),
      update_group_description_policy: get_policy(XmtpMetadataField::Description.as_str()),
      update_group_image_url_square_policy: get_policy(
        XmtpMetadataField::GroupImageUrlSquare.as_str(),
      ),
      update_message_disappearing_policy: get_policy(
        XmtpMetadataField::MessageDisappearInNS.as_str(),
      ),
      update_app_data_policy: get_policy(XmtpMetadataField::AppData.as_str()),
    })
  }
}

impl TryFrom<PermissionPolicySet> for PolicySet {
  type Error = GroupMutablePermissionsError;
  fn try_from(
    policy_set: PermissionPolicySet,
  ) -> std::result::Result<Self, GroupMutablePermissionsError> {
    let mut metadata_permissions_map: HashMap<String, MetadataPolicies> = HashMap::new();
    metadata_permissions_map.insert(
      XmtpMetadataField::GroupName.to_string(),
      policy_set.update_group_name_policy.try_into()?,
    );
    metadata_permissions_map.insert(
      XmtpMetadataField::Description.to_string(),
      policy_set.update_group_description_policy.try_into()?,
    );
    metadata_permissions_map.insert(
      XmtpMetadataField::GroupImageUrlSquare.to_string(),
      policy_set.update_group_image_url_square_policy.try_into()?,
    );
    metadata_permissions_map.insert(
      XmtpMetadataField::MessageDisappearInNS.to_string(),
      policy_set.update_message_disappearing_policy.try_into()?,
    );
    metadata_permissions_map.insert(
      XmtpMetadataField::AppData.to_string(),
      policy_set.update_app_data_policy.try_into()?,
    );

    Ok(PolicySet {
      add_member_policy: policy_set.add_member_policy.try_into()?,
      remove_member_policy: policy_set.remove_member_policy.try_into()?,
      add_admin_policy: policy_set.add_admin_policy.try_into()?,
      remove_admin_policy: policy_set.remove_admin_policy.try_into()?,
      update_metadata_policy: metadata_permissions_map,
      update_permissions_policy: PermissionsPolicies::allow_if_actor_super_admin(),
    })
  }
}

#[napi]
pub enum MetadataField {
  AppData,
  Description,
  GroupName,
  GroupImageUrlSquare,
  MessageExpirationFromNs,
  MessageExpirationInNs,
}

impl From<&MetadataField> for XmtpMetadataField {
  fn from(field: &MetadataField) -> Self {
    match field {
      MetadataField::AppData => XmtpMetadataField::AppData,
      MetadataField::Description => XmtpMetadataField::Description,
      MetadataField::GroupName => XmtpMetadataField::GroupName,
      MetadataField::GroupImageUrlSquare => XmtpMetadataField::GroupImageUrlSquare,
      MetadataField::MessageExpirationFromNs => XmtpMetadataField::MessageDisappearFromNS,
      MetadataField::MessageExpirationInNs => XmtpMetadataField::MessageDisappearInNS,
    }
  }
}

#[napi]
#[allow(dead_code)]
pub fn metadata_field_name(field: MetadataField) -> String {
  XmtpMetadataField::from(&field).as_str().to_string()
}
