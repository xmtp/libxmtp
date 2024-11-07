use std::sync::Arc;
use wasm_bindgen::JsValue;
use wasm_bindgen::{prelude::wasm_bindgen, JsError};

use crate::client::RustXmtpClient;
use crate::encoded_content::EncodedContent;
use crate::messages::{ListMessagesOptions, Message};
use crate::{consent_state::ConsentState, permissions::GroupPermissions};
use xmtp_cryptography::signature::ed25519_public_key_to_address;
use xmtp_mls::groups::{
  group_metadata::{ConversationType, GroupMetadata as XmtpGroupMetadata},
  members::PermissionLevel as XmtpPermissionLevel,
  MlsGroup, UpdateAdminListType,
};
use xmtp_proto::xmtp::mls::message_contents::EncodedContent as XmtpEncodedContent;

use prost::Message as ProstMessage;

#[wasm_bindgen]
pub struct GroupMetadata {
  inner: XmtpGroupMetadata,
}

#[wasm_bindgen]
impl GroupMetadata {
  #[wasm_bindgen(js_name = creatorInboxId)]
  pub fn creator_inbox_id(&self) -> String {
    self.inner.creator_inbox_id.clone()
  }

  #[wasm_bindgen(js_name = conversationType)]
  pub fn conversation_type(&self) -> String {
    match self.inner.conversation_type {
      ConversationType::Group => "group".to_string(),
      ConversationType::Dm => "dm".to_string(),
      ConversationType::Sync => "sync".to_string(),
    }
  }
}

#[wasm_bindgen]
#[derive(Clone, serde::Serialize)]
pub enum PermissionLevel {
  Member,
  Admin,
  SuperAdmin,
}

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone, serde::Serialize)]
pub struct GroupMember {
  #[wasm_bindgen(js_name = inboxId)]
  #[serde(rename = "inboxId")]
  pub inbox_id: String,
  #[wasm_bindgen(js_name = accountAddresses)]
  #[serde(rename = "accountAddresses")]
  pub account_addresses: Vec<String>,
  #[wasm_bindgen(js_name = installationIds)]
  #[serde(rename = "installationIds")]
  pub installation_ids: Vec<String>,
  #[wasm_bindgen(js_name = permissionLevel)]
  #[serde(rename = "permissionLevel")]
  pub permission_level: PermissionLevel,
  #[wasm_bindgen(js_name = consentState)]
  #[serde(rename = "consentState")]
  pub consent_state: ConsentState,
}

#[wasm_bindgen]
impl GroupMember {
  #[wasm_bindgen(constructor)]
  pub fn new(
    inbox_id: String,
    account_addresses: Vec<String>,
    installation_ids: Vec<String>,
    permission_level: PermissionLevel,
    consent_state: ConsentState,
  ) -> Self {
    Self {
      inbox_id,
      account_addresses,
      installation_ids,
      permission_level,
      consent_state,
    }
  }
}

#[wasm_bindgen]
pub struct Conversation {
  inner_client: Arc<RustXmtpClient>,
  group_id: Vec<u8>,
  created_at_ns: i64,
}

impl Conversation {
  pub fn new(inner_client: Arc<RustXmtpClient>, group_id: Vec<u8>, created_at_ns: i64) -> Self {
    Self {
      inner_client,
      group_id,
      created_at_ns,
    }
  }

  pub fn to_mls_group(&self) -> MlsGroup<Arc<RustXmtpClient>> {
    MlsGroup::new(
      self.inner_client.clone(),
      self.group_id.clone(),
      self.created_at_ns,
    )
  }
}

impl From<MlsGroup<RustXmtpClient>> for Conversation {
  fn from(mls_group: MlsGroup<RustXmtpClient>) -> Self {
    Conversation {
      inner_client: mls_group.client,
      group_id: mls_group.group_id,
      created_at_ns: mls_group.created_at_ns,
    }
  }
}

#[wasm_bindgen]
impl Conversation {
  #[wasm_bindgen]
  pub fn id(&self) -> String {
    hex::encode(self.group_id.clone())
  }

  #[wasm_bindgen]
  pub async fn send(&self, encoded_content: EncodedContent) -> Result<String, JsError> {
    let encoded_content: XmtpEncodedContent = encoded_content.into();
    let group = self.to_mls_group();

    let message_id = group
      .send_message(encoded_content.encode_to_vec().as_slice())
      .await
      .map_err(|e| JsError::new(&format!("{e}")))?;

    Ok(hex::encode(message_id.clone()))
  }

  /// send a message without immediately publishing to the delivery service.
  #[wasm_bindgen(js_name = sendOptimistic)]
  pub fn send_optimistic(&self, encoded_content: EncodedContent) -> Result<String, JsError> {
    let encoded_content: XmtpEncodedContent = encoded_content.into();
    let group = self.to_mls_group();

    let id = group
      .send_message_optimistic(encoded_content.encode_to_vec().as_slice())
      .map_err(|e| JsError::new(&format!("{e}")))?;

    Ok(hex::encode(id.clone()))
  }

  /// Publish all unpublished messages
  #[wasm_bindgen(js_name = publishMessages)]
  pub async fn publish_messages(&self) -> Result<(), JsError> {
    let group = self.to_mls_group();
    group
      .publish_messages()
      .await
      .map_err(|e| JsError::new(&format!("{e}")))?;

    Ok(())
  }

  #[wasm_bindgen]
  pub async fn sync(&self) -> Result<(), JsError> {
    let group = self.to_mls_group();

    group
      .sync()
      .await
      .map_err(|e| JsError::new(&format!("{e}")))?;

    Ok(())
  }

  #[wasm_bindgen(js_name = findMessages)]
  pub fn find_messages(&self, opts: Option<ListMessagesOptions>) -> Result<Vec<Message>, JsError> {
    let opts = opts.unwrap_or_default();
    let group = self.to_mls_group();
    let messages: Vec<Message> = group
      .find_messages(&opts.into())
      .map_err(|e| JsError::new(&format!("{e}")))?
      .into_iter()
      .map(Into::into)
      .collect();

    Ok(messages)
  }

  #[wasm_bindgen(js_name = listMembers)]
  pub async fn list_members(&self) -> Result<JsValue, JsError> {
    let group = self.to_mls_group();
    let members: Vec<GroupMember> = group
      .members()
      .await
      .map_err(|e| JsError::new(&format!("{e}")))?
      .into_iter()
      .map(|member| GroupMember {
        inbox_id: member.inbox_id,
        account_addresses: member.account_addresses,
        installation_ids: member
          .installation_ids
          .into_iter()
          .map(|id| ed25519_public_key_to_address(id.as_slice()))
          .collect(),
        permission_level: match member.permission_level {
          XmtpPermissionLevel::Member => PermissionLevel::Member,
          XmtpPermissionLevel::Admin => PermissionLevel::Admin,
          XmtpPermissionLevel::SuperAdmin => PermissionLevel::SuperAdmin,
        },
        consent_state: member.consent_state.into(),
      })
      .collect();

    Ok(serde_wasm_bindgen::to_value(&members)?)
  }

  #[wasm_bindgen(js_name = adminList)]
  pub fn admin_list(&self) -> Result<Vec<String>, JsError> {
    let group = self.to_mls_group();
    let admin_list = group
      .admin_list(
        group
          .mls_provider()
          .map_err(|e| JsError::new(&format!("{e}")))?,
      )
      .map_err(|e| JsError::new(&format!("{e}")))?;

    Ok(admin_list)
  }

  #[wasm_bindgen(js_name = superAdminList)]
  pub fn super_admin_list(&self) -> Result<Vec<String>, JsError> {
    let group = self.to_mls_group();
    let super_admin_list = group
      .super_admin_list(
        group
          .mls_provider()
          .map_err(|e| JsError::new(&format!("{e}")))?,
      )
      .map_err(|e| JsError::new(&format!("{e}")))?;

    Ok(super_admin_list)
  }

  #[wasm_bindgen(js_name = isAdmin)]
  pub fn is_admin(&self, inbox_id: String) -> Result<bool, JsError> {
    let admin_list = self.admin_list()?;
    Ok(admin_list.contains(&inbox_id))
  }

  #[wasm_bindgen(js_name = isSuperAdmin)]
  pub fn is_super_admin(&self, inbox_id: String) -> Result<bool, JsError> {
    let super_admin_list = self.super_admin_list()?;
    Ok(super_admin_list.contains(&inbox_id))
  }

  #[wasm_bindgen(js_name = addMembers)]
  pub async fn add_members(&self, account_addresses: Vec<String>) -> Result<(), JsError> {
    let group = self.to_mls_group();

    group
      .add_members(&account_addresses)
      .await
      .map_err(|e| JsError::new(&format!("{e}")))?;

    Ok(())
  }

  #[wasm_bindgen(js_name = addAdmin)]
  pub async fn add_admin(&self, inbox_id: String) -> Result<(), JsError> {
    let group = self.to_mls_group();
    group
      .update_admin_list(UpdateAdminListType::Add, inbox_id)
      .await
      .map_err(|e| JsError::new(&format!("{e}")))?;

    Ok(())
  }

  #[wasm_bindgen(js_name = removeAdmin)]
  pub async fn remove_admin(&self, inbox_id: String) -> Result<(), JsError> {
    let group = self.to_mls_group();

    group
      .update_admin_list(UpdateAdminListType::Remove, inbox_id)
      .await
      .map_err(|e| JsError::new(&format!("{e}")))?;

    Ok(())
  }

  #[wasm_bindgen(js_name = addSuperAdmin)]
  pub async fn add_super_admin(&self, inbox_id: String) -> Result<(), JsError> {
    let group = self.to_mls_group();

    group
      .update_admin_list(UpdateAdminListType::AddSuper, inbox_id)
      .await
      .map_err(|e| JsError::new(&format!("{e}")))?;

    Ok(())
  }

  #[wasm_bindgen(js_name = removeSuperAdmin)]
  pub async fn remove_super_admin(&self, inbox_id: String) -> Result<(), JsError> {
    let group = self.to_mls_group();

    group
      .update_admin_list(UpdateAdminListType::RemoveSuper, inbox_id)
      .await
      .map_err(|e| JsError::new(&format!("{e}")))?;

    Ok(())
  }

  #[wasm_bindgen(js_name = groupPermissions)]
  pub fn group_permissions(&self) -> Result<GroupPermissions, JsError> {
    let group = self.to_mls_group();

    let permissions = group
      .permissions()
      .map_err(|e| JsError::new(&format!("{e}")))?;

    Ok(GroupPermissions::new(permissions))
  }

  #[wasm_bindgen(js_name = addMembersByInboxId)]
  pub async fn add_members_by_inbox_id(&self, inbox_ids: Vec<String>) -> Result<(), JsError> {
    let group = self.to_mls_group();

    group
      .add_members_by_inbox_id(&inbox_ids)
      .await
      .map_err(|e| JsError::new(&format!("{e}")))?;

    Ok(())
  }

  #[wasm_bindgen(js_name = removeMembers)]
  pub async fn remove_members(&self, account_addresses: Vec<String>) -> Result<(), JsError> {
    let group = self.to_mls_group();

    group
      .remove_members(&account_addresses)
      .await
      .map_err(|e| JsError::new(&format!("{e}")))?;

    Ok(())
  }

  #[wasm_bindgen(js_name = removeMembersByInboxId)]
  pub async fn remove_members_by_inbox_id(&self, inbox_ids: Vec<String>) -> Result<(), JsError> {
    let group = self.to_mls_group();

    let ids = inbox_ids.iter().map(AsRef::as_ref).collect::<Vec<&str>>();
    group
      .remove_members_by_inbox_id(ids.as_slice())
      .await
      .map_err(|e| JsError::new(&format!("{e}")))?;

    Ok(())
  }

  #[wasm_bindgen(js_name = updateGroupName)]
  pub async fn update_group_name(&self, group_name: String) -> Result<(), JsError> {
    let group = self.to_mls_group();

    group
      .update_group_name(group_name)
      .await
      .map_err(|e| JsError::new(&format!("{e}")))?;

    Ok(())
  }

  #[wasm_bindgen(js_name = groupName)]
  pub fn group_name(&self) -> Result<String, JsError> {
    let group = self.to_mls_group();

    let group_name = group
      .group_name(
        group
          .mls_provider()
          .map_err(|e| JsError::new(&format!("{e}")))?,
      )
      .map_err(|e| JsError::new(&format!("{e}")))?;

    Ok(group_name)
  }

  #[wasm_bindgen(js_name = updateGroupImageUrlSquare)]
  pub async fn update_group_image_url_square(
    &self,
    group_image_url_square: String,
  ) -> Result<(), JsError> {
    let group = self.to_mls_group();

    group
      .update_group_image_url_square(group_image_url_square)
      .await
      .map_err(|e| JsError::new(&format!("{e}")))?;

    Ok(())
  }

  #[wasm_bindgen(js_name = groupImageUrlSquare)]
  pub fn group_image_url_square(&self) -> Result<String, JsError> {
    let group = self.to_mls_group();

    let group_image_url_square = group
      .group_image_url_square(
        group
          .mls_provider()
          .map_err(|e| JsError::new(&format!("{e}")))?,
      )
      .map_err(|e| JsError::new(&format!("{e}")))?;

    Ok(group_image_url_square)
  }

  #[wasm_bindgen(js_name = updateGroupDescription)]
  pub async fn update_group_description(&self, group_description: String) -> Result<(), JsError> {
    let group = self.to_mls_group();

    group
      .update_group_description(group_description)
      .await
      .map_err(|e| JsError::new(&format!("{e}")))?;

    Ok(())
  }

  #[wasm_bindgen(js_name = groupDescription)]
  pub fn group_description(&self) -> Result<String, JsError> {
    let group = self.to_mls_group();

    let group_description = group
      .group_description(
        group
          .mls_provider()
          .map_err(|e| JsError::new(&format!("{e}")))?,
      )
      .map_err(|e| JsError::new(&format!("{e}")))?;

    Ok(group_description)
  }

  #[wasm_bindgen(js_name = updateGroupPinnedFrameUrl)]
  pub async fn update_group_pinned_frame_url(
    &self,
    pinned_frame_url: String,
  ) -> Result<(), JsError> {
    let group = self.to_mls_group();

    group
      .update_group_pinned_frame_url(pinned_frame_url)
      .await
      .map_err(|e| JsError::new(&format!("{e}")))?;

    Ok(())
  }

  #[wasm_bindgen(js_name = groupPinnedFrameUrl)]
  pub fn group_pinned_frame_url(&self) -> Result<String, JsError> {
    let group = self.to_mls_group();

    let group_pinned_frame_url = group
      .group_pinned_frame_url(
        group
          .mls_provider()
          .map_err(|e| JsError::new(&format!("{e}")))?,
      )
      .map_err(|e| JsError::new(&format!("{e}")))?;

    Ok(group_pinned_frame_url)
  }

  #[wasm_bindgen(js_name = createdAtNs)]
  pub fn created_at_ns(&self) -> i64 {
    self.created_at_ns
  }

  #[wasm_bindgen(js_name = isActive)]
  pub fn is_active(&self) -> Result<bool, JsError> {
    let group = self.to_mls_group();

    group
      .is_active(
        group
          .mls_provider()
          .map_err(|e| JsError::new(&format!("{e}")))?,
      )
      .map_err(|e| JsError::new(&format!("{e}")))
  }

  #[wasm_bindgen(js_name = addedByInboxId)]
  pub fn added_by_inbox_id(&self) -> Result<String, JsError> {
    let group = self.to_mls_group();

    group
      .added_by_inbox_id()
      .map_err(|e| JsError::new(&format!("{e}")))
  }

  #[wasm_bindgen(js_name = groupMetadata)]
  pub fn group_metadata(&self) -> Result<GroupMetadata, JsError> {
    let group = self.to_mls_group();
    let metadata = group
      .metadata(
        group
          .mls_provider()
          .map_err(|e| JsError::new(&format!("{e}")))?,
      )
      .map_err(|e| JsError::new(&format!("{e}")))?;

    Ok(GroupMetadata { inner: metadata })
  }

  #[wasm_bindgen(js_name = dmPeerInboxId)]
  pub fn dm_peer_inbox_id(&self) -> Result<String, JsError> {
    let group = self.to_mls_group();

    group
      .dm_inbox_id()
      .map_err(|e| JsError::new(&format!("{e}")))
  }
}
