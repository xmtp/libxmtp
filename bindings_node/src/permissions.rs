use napi::bindgen_prelude::{Error, Result};
use napi_derive::napi;
use xmtp_mls::groups::{
  group_mutable_metadata::MetadataField,
  group_permissions::{
    BasePolicies, GroupMutablePermissions, MembershipPolicies, MetadataBasePolicies,
    MetadataPolicies, PermissionsBasePolicies, PermissionsPolicies,
  },
  intents::{PermissionPolicyOption, PermissionUpdateType},
  PreconfiguredPolicies,
};

#[napi]
pub enum NapiGroupPermissionsOptions {
  AllMembers,
  AdminOnly,
  CustomPolicy,
}

#[napi]
pub enum NapiPermissionUpdateType {
  AddMember,
  RemoveMember,
  AddAdmin,
  RemoveAdmin,
  UpdateMetadata,
}

impl From<&NapiPermissionUpdateType> for PermissionUpdateType {
  fn from(update_type: &NapiPermissionUpdateType) -> Self {
    match update_type {
      NapiPermissionUpdateType::AddMember => PermissionUpdateType::AddMember,
      NapiPermissionUpdateType::RemoveMember => PermissionUpdateType::RemoveMember,
      NapiPermissionUpdateType::AddAdmin => PermissionUpdateType::AddAdmin,
      NapiPermissionUpdateType::RemoveAdmin => PermissionUpdateType::RemoveAdmin,
      NapiPermissionUpdateType::UpdateMetadata => PermissionUpdateType::UpdateMetadata,
    }
  }
}

#[napi]
pub enum NapiPermissionPolicy {
  Allow,
  Deny,
  Admin,
  SuperAdmin,
  DoesNotExist,
  Other,
}

impl TryInto<PermissionPolicyOption> for NapiPermissionPolicy {
  type Error = Error;

  fn try_into(self) -> Result<PermissionPolicyOption> {
    match self {
      NapiPermissionPolicy::Allow => Ok(PermissionPolicyOption::Allow),
      NapiPermissionPolicy::Deny => Ok(PermissionPolicyOption::Deny),
      NapiPermissionPolicy::Admin => Ok(PermissionPolicyOption::AdminOnly),
      NapiPermissionPolicy::SuperAdmin => Ok(PermissionPolicyOption::SuperAdminOnly),
      _ => Err(Error::from_reason("InvalidPermissionPolicyOption")),
    }
  }
}

impl From<&MembershipPolicies> for NapiPermissionPolicy {
  fn from(policies: &MembershipPolicies) -> Self {
    if let MembershipPolicies::Standard(base_policy) = policies {
      match base_policy {
        BasePolicies::Allow => NapiPermissionPolicy::Allow,
        BasePolicies::Deny => NapiPermissionPolicy::Deny,
        BasePolicies::AllowSameMember => NapiPermissionPolicy::Other,
        BasePolicies::AllowIfAdminOrSuperAdmin => NapiPermissionPolicy::Admin,
        BasePolicies::AllowIfSuperAdmin => NapiPermissionPolicy::SuperAdmin,
      }
    } else {
      NapiPermissionPolicy::Other
    }
  }
}

impl From<&MetadataPolicies> for NapiPermissionPolicy {
  fn from(policies: &MetadataPolicies) -> Self {
    if let MetadataPolicies::Standard(base_policy) = policies {
      match base_policy {
        MetadataBasePolicies::Allow => NapiPermissionPolicy::Allow,
        MetadataBasePolicies::Deny => NapiPermissionPolicy::Deny,
        MetadataBasePolicies::AllowIfActorAdminOrSuperAdmin => NapiPermissionPolicy::Admin,
        MetadataBasePolicies::AllowIfActorSuperAdmin => NapiPermissionPolicy::SuperAdmin,
      }
    } else {
      NapiPermissionPolicy::Other
    }
  }
}

impl From<&PermissionsPolicies> for NapiPermissionPolicy {
  fn from(policies: &PermissionsPolicies) -> Self {
    if let PermissionsPolicies::Standard(base_policy) = policies {
      match base_policy {
        PermissionsBasePolicies::Deny => NapiPermissionPolicy::Deny,
        PermissionsBasePolicies::AllowIfActorAdminOrSuperAdmin => NapiPermissionPolicy::Admin,
        PermissionsBasePolicies::AllowIfActorSuperAdmin => NapiPermissionPolicy::SuperAdmin,
      }
    } else {
      NapiPermissionPolicy::Other
    }
  }
}

#[napi(object)]
pub struct NapiPermissionPolicySet {
  pub add_member_policy: NapiPermissionPolicy,
  pub remove_member_policy: NapiPermissionPolicy,
  pub add_admin_policy: NapiPermissionPolicy,
  pub remove_admin_policy: NapiPermissionPolicy,
  pub update_group_name_policy: NapiPermissionPolicy,
  pub update_group_description_policy: NapiPermissionPolicy,
  pub update_group_image_url_square_policy: NapiPermissionPolicy,
  pub update_group_pinned_frame_url_policy: NapiPermissionPolicy,
}

impl From<PreconfiguredPolicies> for NapiGroupPermissionsOptions {
  fn from(policy: PreconfiguredPolicies) -> Self {
    match policy {
      PreconfiguredPolicies::AllMembers => NapiGroupPermissionsOptions::AllMembers,
      PreconfiguredPolicies::AdminsOnly => NapiGroupPermissionsOptions::AdminOnly,
    }
  }
}

#[napi]
pub struct NapiGroupPermissions {
  inner: GroupMutablePermissions,
}

#[napi]
impl NapiGroupPermissions {
  pub fn new(permissions: GroupMutablePermissions) -> Self {
    Self { inner: permissions }
  }

  #[napi]
  pub fn policy_type(&self) -> Result<NapiGroupPermissionsOptions> {
    if let Ok(preconfigured_policy) = self.inner.preconfigured_policy() {
      Ok(preconfigured_policy.into())
    } else {
      Ok(NapiGroupPermissionsOptions::CustomPolicy)
    }
  }

  #[napi]
  pub fn policy_set(&self) -> Result<NapiPermissionPolicySet> {
    let policy_set = &self.inner.policies;
    let metadata_policy_map = &policy_set.update_metadata_policy;
    let get_policy = |field: &str| {
      metadata_policy_map
        .get(field)
        .map(NapiPermissionPolicy::from)
        .unwrap_or(NapiPermissionPolicy::DoesNotExist)
    };
    Ok(NapiPermissionPolicySet {
      add_member_policy: NapiPermissionPolicy::from(&policy_set.add_member_policy),
      remove_member_policy: NapiPermissionPolicy::from(&policy_set.remove_member_policy),
      add_admin_policy: NapiPermissionPolicy::from(&policy_set.add_admin_policy),
      remove_admin_policy: NapiPermissionPolicy::from(&policy_set.remove_admin_policy),
      update_group_name_policy: get_policy(MetadataField::GroupName.as_str()),
      update_group_description_policy: get_policy(MetadataField::Description.as_str()),
      update_group_image_url_square_policy: get_policy(MetadataField::GroupImageUrlSquare.as_str()),
      update_group_pinned_frame_url_policy: get_policy(MetadataField::GroupPinnedFrameUrl.as_str()),
    })
  }
}
