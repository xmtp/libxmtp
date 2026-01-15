use crate::conversation::Conversation;
use crate::{
  ErrorWrapper,
  consent_state::ConsentState,
  identity::{Identifier, IdentityExt},
  permissions::GroupPermissions,
};
use napi::bindgen_prelude::Result;
use napi_derive::napi;
use xmtp_db::group::GroupMembershipState as XmtpGroupMembershipState;
use xmtp_mls::groups::{UpdateAdminListType, members::PermissionLevel as XmtpPermissionLevel};

#[napi]
pub enum PermissionLevel {
  Member,
  Admin,
  SuperAdmin,
}

#[napi]
#[derive(Debug)]
pub enum GroupMembershipState {
  Allowed = 0,
  Rejected = 1,
  Pending = 2,
  Restored = 3,
  PendingRemove = 4,
}

impl From<XmtpGroupMembershipState> for GroupMembershipState {
  fn from(gms: XmtpGroupMembershipState) -> Self {
    match gms {
      XmtpGroupMembershipState::Allowed => GroupMembershipState::Allowed,
      XmtpGroupMembershipState::Rejected => GroupMembershipState::Rejected,
      XmtpGroupMembershipState::Pending => GroupMembershipState::Pending,
      XmtpGroupMembershipState::Restored => GroupMembershipState::Restored,
      XmtpGroupMembershipState::PendingRemove => GroupMembershipState::PendingRemove,
    }
  }
}

impl From<GroupMembershipState> for XmtpGroupMembershipState {
  fn from(gms: GroupMembershipState) -> Self {
    match gms {
      GroupMembershipState::Allowed => XmtpGroupMembershipState::Allowed,
      GroupMembershipState::Rejected => XmtpGroupMembershipState::Rejected,
      GroupMembershipState::Pending => XmtpGroupMembershipState::Pending,
      GroupMembershipState::Restored => XmtpGroupMembershipState::Restored,
      GroupMembershipState::PendingRemove => XmtpGroupMembershipState::PendingRemove,
    }
  }
}

#[napi]
pub struct GroupMember {
  pub inbox_id: String,
  account_identifiers: Vec<Identifier>,
  pub installation_ids: Vec<String>,
  pub permission_level: PermissionLevel,
  pub consent_state: ConsentState,
}

#[napi]
impl GroupMember {
  #[napi(getter)]
  pub fn account_identifiers(&self) -> Vec<Identifier> {
    self.account_identifiers.clone()
  }
}

#[napi]
impl Conversation {
  #[napi]
  pub async fn list_members(&self) -> Result<Vec<GroupMember>> {
    let group = self.create_mls_group();

    let members: Vec<GroupMember> = group
      .members()
      .await
      .map_err(ErrorWrapper::from)?
      .into_iter()
      .map(|member| GroupMember {
        inbox_id: member.inbox_id,
        account_identifiers: member
          .account_identifiers
          .into_iter()
          .map(Into::into)
          .collect(),
        installation_ids: member
          .installation_ids
          .into_iter()
          .map(hex::encode)
          .collect(),
        permission_level: match member.permission_level {
          XmtpPermissionLevel::Member => PermissionLevel::Member,
          XmtpPermissionLevel::Admin => PermissionLevel::Admin,
          XmtpPermissionLevel::SuperAdmin => PermissionLevel::SuperAdmin,
        },
        consent_state: member.consent_state.into(),
      })
      .collect();

    Ok(members)
  }

  #[napi]
  pub fn membership_state(&self) -> Result<GroupMembershipState> {
    let group = self.create_mls_group();
    let state = group.membership_state().map_err(ErrorWrapper::from)?;
    Ok(state.into())
  }

  #[napi]
  pub fn admin_list(&self) -> Result<Vec<String>> {
    let group = self.create_mls_group();

    let admin_list = group.admin_list().map_err(ErrorWrapper::from)?;

    Ok(admin_list)
  }

  #[napi]
  pub fn super_admin_list(&self) -> Result<Vec<String>> {
    let group = self.create_mls_group();

    let super_admin_list = group.super_admin_list().map_err(ErrorWrapper::from)?;

    Ok(super_admin_list)
  }

  #[napi]
  pub fn is_admin(&self, inbox_id: String) -> Result<bool> {
    let admin_list = self.admin_list().map_err(ErrorWrapper::from)?;
    Ok(admin_list.contains(&inbox_id))
  }

  #[napi]
  pub fn is_super_admin(&self, inbox_id: String) -> Result<bool> {
    let super_admin_list = self.super_admin_list().map_err(ErrorWrapper::from)?;
    Ok(super_admin_list.contains(&inbox_id))
  }

  #[napi]
  pub async fn add_members(&self, account_identities: Vec<Identifier>) -> Result<()> {
    let group = self.create_mls_group();

    group
      .add_members(&account_identities.to_internal()?)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }

  #[napi]
  pub async fn add_admin(&self, inbox_id: String) -> Result<()> {
    let group = self.create_mls_group();
    group
      .update_admin_list(UpdateAdminListType::Add, inbox_id)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }

  #[napi]
  pub async fn remove_admin(&self, inbox_id: String) -> Result<()> {
    let group = self.create_mls_group();
    group
      .update_admin_list(UpdateAdminListType::Remove, inbox_id)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }

  #[napi]
  pub async fn add_super_admin(&self, inbox_id: String) -> Result<()> {
    let group = self.create_mls_group();
    group
      .update_admin_list(UpdateAdminListType::AddSuper, inbox_id)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }

  #[napi]
  pub async fn remove_super_admin(&self, inbox_id: String) -> Result<()> {
    let group = self.create_mls_group();
    group
      .update_admin_list(UpdateAdminListType::RemoveSuper, inbox_id)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }

  #[napi]
  pub fn group_permissions(&self) -> Result<GroupPermissions> {
    let group = self.create_mls_group();

    let permissions = group.permissions().map_err(ErrorWrapper::from)?;

    Ok(GroupPermissions::new(permissions))
  }

  #[napi]
  pub async fn add_members_by_inbox_id(&self, inbox_ids: Vec<String>) -> Result<()> {
    let group = self.create_mls_group();

    group
      .add_members_by_inbox_id(&inbox_ids)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }

  #[napi]
  pub async fn remove_members(&self, account_identities: Vec<Identifier>) -> Result<()> {
    let group = self.create_mls_group();

    group
      .remove_members(&account_identities.to_internal()?)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }

  #[napi]
  pub async fn remove_members_by_inbox_id(&self, inbox_ids: Vec<String>) -> Result<()> {
    let group = self.create_mls_group();

    group
      .remove_members_by_inbox_id(
        inbox_ids
          .iter()
          .map(AsRef::as_ref)
          .collect::<Vec<&str>>()
          .as_slice(),
      )
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }

  #[napi]
  pub fn added_by_inbox_id(&self) -> Result<String> {
    let group = self.create_mls_group();

    Ok(group.added_by_inbox_id().map_err(ErrorWrapper::from)?)
  }

  #[napi]
  pub async fn leave_group(&self) -> Result<()> {
    let group = self.create_mls_group();
    group.leave_group().await.map_err(ErrorWrapper::from)?;
    Ok(())
  }
}
