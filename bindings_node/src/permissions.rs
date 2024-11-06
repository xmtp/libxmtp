use napi::bindgen_prelude::{Error, Result};
use napi_derive::napi;
use xmtp_mls::groups::{
  group_mutable_metadata::MetadataField,
  group_permissions::{
    BasePolicies, GroupMutablePermissions, MembershipPolicies, MetadataBasePolicies,
    MetadataPolicies, PermissionsBasePolicies, PermissionsPolicies,
  },
  intents::{PermissionPolicyOption, PermissionUpdateType as XmtpPermissionUpdateType},
  PreconfiguredPolicies,
};

#[napi]
pub enum GroupPermissionsOptions {
  AllMembers,
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
pub enum PermissionPolicy {
  Allow,
  Deny,
  Admin,
  SuperAdmin,
  DoesNotExist,
  Other,
}

impl TryInto<PermissionPolicyOption> for PermissionPolicy {
  type Error = Error;

  fn try_into(self) -> Result<PermissionPolicyOption> {
    match self {
      PermissionPolicy::Allow => Ok(PermissionPolicyOption::Allow),
      PermissionPolicy::Deny => Ok(PermissionPolicyOption::Deny),
      PermissionPolicy::Admin => Ok(PermissionPolicyOption::AdminOnly),
      PermissionPolicy::SuperAdmin => Ok(PermissionPolicyOption::SuperAdminOnly),
      _ => Err(Error::from_reason("InvalidPermissionPolicyOption")),
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

#[napi(object)]
pub struct PermissionPolicySet {
  pub add_member_policy: PermissionPolicy,
  pub remove_member_policy: PermissionPolicy,
  pub add_admin_policy: PermissionPolicy,
  pub remove_admin_policy: PermissionPolicy,
  pub update_group_name_policy: PermissionPolicy,
  pub update_group_description_policy: PermissionPolicy,
  pub update_group_image_url_square_policy: PermissionPolicy,
  pub update_group_pinned_frame_url_policy: PermissionPolicy,
}

impl From<PreconfiguredPolicies> for GroupPermissionsOptions {
  fn from(policy: PreconfiguredPolicies) -> Self {
    match policy {
      PreconfiguredPolicies::AllMembers => GroupPermissionsOptions::AllMembers,
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
      update_group_name_policy: get_policy(MetadataField::GroupName.as_str()),
      update_group_description_policy: get_policy(MetadataField::Description.as_str()),
      update_group_image_url_square_policy: get_policy(MetadataField::GroupImageUrlSquare.as_str()),
      update_group_pinned_frame_url_policy: get_policy(MetadataField::GroupPinnedFrameUrl.as_str()),
    })
  }
}
