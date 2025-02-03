use std::collections::HashMap;
use wasm_bindgen::{prelude::wasm_bindgen, JsError};
use xmtp_mls::groups::{
  group_mutable_metadata::MetadataField as XmtpMetadataField,
  group_permissions::{
    BasePolicies, GroupMutablePermissions, GroupMutablePermissionsError, MembershipPolicies,
    MetadataBasePolicies, MetadataPolicies, PermissionsBasePolicies, PermissionsPolicies,
    PolicySet,
  },
  intents::{PermissionPolicyOption, PermissionUpdateType as XmtpPermissionUpdateType},
  PreconfiguredPolicies,
};

#[wasm_bindgen]
#[derive(Clone)]
pub enum GroupPermissionsOptions {
  Default,
  AdminOnly,
  CustomPolicy,
}

#[wasm_bindgen]
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

#[wasm_bindgen]
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
  type Error = JsError;

  fn try_into(self) -> Result<PermissionPolicyOption, JsError> {
    match self {
      PermissionPolicy::Allow => Ok(PermissionPolicyOption::Allow),
      PermissionPolicy::Deny => Ok(PermissionPolicyOption::Deny),
      PermissionPolicy::Admin => Ok(PermissionPolicyOption::AdminOnly),
      PermissionPolicy::SuperAdmin => Ok(PermissionPolicyOption::SuperAdminOnly),
      _ => Err(JsError::new("InvalidPermissionPolicyOption")),
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

  fn try_into(self) -> Result<MetadataPolicies, Self::Error> {
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

  fn try_into(self) -> Result<PermissionsPolicies, Self::Error> {
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

  fn try_into(self) -> Result<MembershipPolicies, Self::Error> {
    match self {
      PermissionPolicy::Allow => Ok(MembershipPolicies::allow()),
      PermissionPolicy::Deny => Ok(MembershipPolicies::deny()),
      PermissionPolicy::Admin => Ok(MembershipPolicies::allow_if_actor_admin()),
      PermissionPolicy::SuperAdmin => Ok(MembershipPolicies::allow_if_actor_super_admin()),
      _ => Err(GroupMutablePermissionsError::InvalidPermissionPolicyOption),
    }
  }
}

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone)]
pub struct PermissionPolicySet {
  #[wasm_bindgen(js_name = addMemberPolicy)]
  pub add_member_policy: PermissionPolicy,
  #[wasm_bindgen(js_name = removeMemberPolicy)]
  pub remove_member_policy: PermissionPolicy,
  #[wasm_bindgen(js_name = addAdminPolicy)]
  pub add_admin_policy: PermissionPolicy,
  #[wasm_bindgen(js_name = removeAdminPolicy)]
  pub remove_admin_policy: PermissionPolicy,
  #[wasm_bindgen(js_name = updateGroupNamePolicy)]
  pub update_group_name_policy: PermissionPolicy,
  #[wasm_bindgen(js_name = updateGroupDescriptionPolicy)]
  pub update_group_description_policy: PermissionPolicy,
  #[wasm_bindgen(js_name = updateGroupImageUrlSquarePolicy)]
  pub update_group_image_url_square_policy: PermissionPolicy,
  #[wasm_bindgen(js_name = updateMessageDisappearingPolicy)]
  pub update_message_disappearing_policy: PermissionPolicy,
}

#[wasm_bindgen]
impl PermissionPolicySet {
  #[wasm_bindgen(constructor)]
  #[allow(clippy::too_many_arguments)]
  pub fn new(
    add_member_policy: PermissionPolicy,
    remove_member_policy: PermissionPolicy,
    add_admin_policy: PermissionPolicy,
    remove_admin_policy: PermissionPolicy,
    update_group_name_policy: PermissionPolicy,
    update_group_description_policy: PermissionPolicy,
    update_group_image_url_square_policy: PermissionPolicy,
    update_message_disappearing_policy: PermissionPolicy,
  ) -> Self {
    Self {
      add_member_policy,
      remove_member_policy,
      add_admin_policy,
      remove_admin_policy,
      update_group_name_policy,
      update_group_description_policy,
      update_group_image_url_square_policy,
      update_message_disappearing_policy,
    }
  }
}

impl From<PreconfiguredPolicies> for GroupPermissionsOptions {
  fn from(policy: PreconfiguredPolicies) -> Self {
    match policy {
      PreconfiguredPolicies::Default => GroupPermissionsOptions::Default,
      PreconfiguredPolicies::AdminsOnly => GroupPermissionsOptions::AdminOnly,
    }
  }
}

#[wasm_bindgen]
pub struct GroupPermissions {
  inner: GroupMutablePermissions,
}

impl GroupPermissions {
  pub fn new(permissions: GroupMutablePermissions) -> Self {
    Self { inner: permissions }
  }
}

#[wasm_bindgen]
impl GroupPermissions {
  #[wasm_bindgen(js_name = policyType)]
  pub fn policy_type(&self) -> Result<GroupPermissionsOptions, JsError> {
    if let Ok(preconfigured_policy) = self.inner.preconfigured_policy() {
      Ok(preconfigured_policy.into())
    } else {
      Ok(GroupPermissionsOptions::CustomPolicy)
    }
  }

  #[wasm_bindgen(js_name = policySet)]
  pub fn policy_set(&self) -> Result<PermissionPolicySet, JsError> {
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
    })
  }
}

impl TryFrom<PermissionPolicySet> for PolicySet {
  type Error = GroupMutablePermissionsError;
  fn try_from(policy_set: PermissionPolicySet) -> Result<Self, GroupMutablePermissionsError> {
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

#[wasm_bindgen]
pub enum MetadataField {
  GroupName,
  Description,
  ImageUrlSquare,
  MessageExpirationFromMS,
  MessageExpirationMS,
}

impl From<&MetadataField> for XmtpMetadataField {
  fn from(field: &MetadataField) -> Self {
    match field {
      MetadataField::GroupName => XmtpMetadataField::GroupName,
      MetadataField::Description => XmtpMetadataField::Description,
      MetadataField::ImageUrlSquare => XmtpMetadataField::GroupImageUrlSquare,
      MetadataField::MessageExpirationFromMS => XmtpMetadataField::MessageDisappearFromNS,
      MetadataField::MessageExpirationMS => XmtpMetadataField::MessageDisappearInNS,
    }
  }
}
