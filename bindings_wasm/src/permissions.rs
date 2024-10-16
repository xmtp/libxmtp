use wasm_bindgen::{prelude::wasm_bindgen, JsError};
use xmtp_mls::groups::{
  group_mutable_metadata::MetadataField,
  group_permissions::{
    BasePolicies, GroupMutablePermissions, MembershipPolicies, MetadataBasePolicies,
    MetadataPolicies, PermissionsBasePolicies, PermissionsPolicies,
  },
  intents::{PermissionPolicyOption, PermissionUpdateType},
  PreconfiguredPolicies,
};

#[wasm_bindgen]
#[derive(Clone)]
pub enum WasmGroupPermissionsOptions {
  AllMembers,
  AdminOnly,
  CustomPolicy,
}

#[wasm_bindgen]
pub enum WasmPermissionUpdateType {
  AddMember,
  RemoveMember,
  AddAdmin,
  RemoveAdmin,
  UpdateMetadata,
}

impl From<&WasmPermissionUpdateType> for PermissionUpdateType {
  fn from(update_type: &WasmPermissionUpdateType) -> Self {
    match update_type {
      WasmPermissionUpdateType::AddMember => PermissionUpdateType::AddMember,
      WasmPermissionUpdateType::RemoveMember => PermissionUpdateType::RemoveMember,
      WasmPermissionUpdateType::AddAdmin => PermissionUpdateType::AddAdmin,
      WasmPermissionUpdateType::RemoveAdmin => PermissionUpdateType::RemoveAdmin,
      WasmPermissionUpdateType::UpdateMetadata => PermissionUpdateType::UpdateMetadata,
    }
  }
}

#[wasm_bindgen]
#[derive(Clone)]
pub enum WasmPermissionPolicy {
  Allow,
  Deny,
  Admin,
  SuperAdmin,
  DoesNotExist,
  Other,
}

impl TryInto<PermissionPolicyOption> for WasmPermissionPolicy {
  type Error = JsError;

  fn try_into(self) -> Result<PermissionPolicyOption, JsError> {
    match self {
      WasmPermissionPolicy::Allow => Ok(PermissionPolicyOption::Allow),
      WasmPermissionPolicy::Deny => Ok(PermissionPolicyOption::Deny),
      WasmPermissionPolicy::Admin => Ok(PermissionPolicyOption::AdminOnly),
      WasmPermissionPolicy::SuperAdmin => Ok(PermissionPolicyOption::SuperAdminOnly),
      _ => Err(JsError::new("InvalidPermissionPolicyOption")),
    }
  }
}

impl From<&MembershipPolicies> for WasmPermissionPolicy {
  fn from(policies: &MembershipPolicies) -> Self {
    if let MembershipPolicies::Standard(base_policy) = policies {
      match base_policy {
        BasePolicies::Allow => WasmPermissionPolicy::Allow,
        BasePolicies::Deny => WasmPermissionPolicy::Deny,
        BasePolicies::AllowSameMember => WasmPermissionPolicy::Other,
        BasePolicies::AllowIfAdminOrSuperAdmin => WasmPermissionPolicy::Admin,
        BasePolicies::AllowIfSuperAdmin => WasmPermissionPolicy::SuperAdmin,
      }
    } else {
      WasmPermissionPolicy::Other
    }
  }
}

impl From<&MetadataPolicies> for WasmPermissionPolicy {
  fn from(policies: &MetadataPolicies) -> Self {
    if let MetadataPolicies::Standard(base_policy) = policies {
      match base_policy {
        MetadataBasePolicies::Allow => WasmPermissionPolicy::Allow,
        MetadataBasePolicies::Deny => WasmPermissionPolicy::Deny,
        MetadataBasePolicies::AllowIfActorAdminOrSuperAdmin => WasmPermissionPolicy::Admin,
        MetadataBasePolicies::AllowIfActorSuperAdmin => WasmPermissionPolicy::SuperAdmin,
      }
    } else {
      WasmPermissionPolicy::Other
    }
  }
}

impl From<&PermissionsPolicies> for WasmPermissionPolicy {
  fn from(policies: &PermissionsPolicies) -> Self {
    if let PermissionsPolicies::Standard(base_policy) = policies {
      match base_policy {
        PermissionsBasePolicies::Deny => WasmPermissionPolicy::Deny,
        PermissionsBasePolicies::AllowIfActorAdminOrSuperAdmin => WasmPermissionPolicy::Admin,
        PermissionsBasePolicies::AllowIfActorSuperAdmin => WasmPermissionPolicy::SuperAdmin,
      }
    } else {
      WasmPermissionPolicy::Other
    }
  }
}

#[wasm_bindgen(getter_with_clone)]
pub struct WasmPermissionPolicySet {
  pub add_member_policy: WasmPermissionPolicy,
  pub remove_member_policy: WasmPermissionPolicy,
  pub add_admin_policy: WasmPermissionPolicy,
  pub remove_admin_policy: WasmPermissionPolicy,
  pub update_group_name_policy: WasmPermissionPolicy,
  pub update_group_description_policy: WasmPermissionPolicy,
  pub update_group_image_url_square_policy: WasmPermissionPolicy,
  pub update_group_pinned_frame_url_policy: WasmPermissionPolicy,
}

impl From<PreconfiguredPolicies> for WasmGroupPermissionsOptions {
  fn from(policy: PreconfiguredPolicies) -> Self {
    match policy {
      PreconfiguredPolicies::AllMembers => WasmGroupPermissionsOptions::AllMembers,
      PreconfiguredPolicies::AdminsOnly => WasmGroupPermissionsOptions::AdminOnly,
    }
  }
}

#[wasm_bindgen]
pub struct WasmGroupPermissions {
  inner: GroupMutablePermissions,
}

impl WasmGroupPermissions {
  pub fn new(permissions: GroupMutablePermissions) -> Self {
    Self { inner: permissions }
  }
}

#[wasm_bindgen]
impl WasmGroupPermissions {
  #[wasm_bindgen]
  #[wasm_bindgen]
  pub fn policy_type(&self) -> Result<WasmGroupPermissionsOptions, JsError> {
    if let Ok(preconfigured_policy) = self.inner.preconfigured_policy() {
      Ok(preconfigured_policy.into())
    } else {
      Ok(WasmGroupPermissionsOptions::CustomPolicy)
    }
  }

  #[wasm_bindgen]
  pub fn policy_set(&self) -> Result<WasmPermissionPolicySet, JsError> {
    let policy_set = &self.inner.policies;
    let metadata_policy_map = &policy_set.update_metadata_policy;
    let get_policy = |field: &str| {
      metadata_policy_map
        .get(field)
        .map(WasmPermissionPolicy::from)
        .unwrap_or(WasmPermissionPolicy::DoesNotExist)
    };
    Ok(WasmPermissionPolicySet {
      add_member_policy: WasmPermissionPolicy::from(&policy_set.add_member_policy),
      remove_member_policy: WasmPermissionPolicy::from(&policy_set.remove_member_policy),
      add_admin_policy: WasmPermissionPolicy::from(&policy_set.add_admin_policy),
      remove_admin_policy: WasmPermissionPolicy::from(&policy_set.remove_admin_policy),
      update_group_name_policy: get_policy(MetadataField::GroupName.as_str()),
      update_group_description_policy: get_policy(MetadataField::Description.as_str()),
      update_group_image_url_square_policy: get_policy(MetadataField::GroupImageUrlSquare.as_str()),
      update_group_pinned_frame_url_policy: get_policy(MetadataField::GroupPinnedFrameUrl.as_str()),
    })
  }
}
