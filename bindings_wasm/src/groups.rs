use std::sync::Arc;
use wasm_bindgen::JsValue;
use wasm_bindgen::{prelude::wasm_bindgen, JsError};
use xmtp_mls::storage::group_message::MsgQueryArgs;

use crate::encoded_content::WasmEncodedContent;
use crate::messages::{WasmListMessagesOptions, WasmMessage};
use crate::mls_client::RustXmtpClient;
use crate::{consent_state::WasmConsentState, permissions::WasmGroupPermissions};
use xmtp_cryptography::signature::ed25519_public_key_to_address;
use xmtp_mls::groups::{
  group_metadata::{ConversationType, GroupMetadata},
  members::PermissionLevel,
  MlsGroup, UpdateAdminListType,
};
use xmtp_proto::xmtp::mls::message_contents::EncodedContent;

use prost::Message;

#[wasm_bindgen]
pub struct WasmGroupMetadata {
  inner: GroupMetadata,
}

#[wasm_bindgen]
impl WasmGroupMetadata {
  #[wasm_bindgen]
  pub fn creator_inbox_id(&self) -> String {
    self.inner.creator_inbox_id.clone()
  }

  #[wasm_bindgen]
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
pub enum WasmPermissionLevel {
  Member,
  Admin,
  SuperAdmin,
}

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone, serde::Serialize)]
pub struct WasmGroupMember {
  pub inbox_id: String,
  pub account_addresses: Vec<String>,
  pub installation_ids: Vec<String>,
  pub permission_level: WasmPermissionLevel,
  pub consent_state: WasmConsentState,
}

#[wasm_bindgen]
impl WasmGroupMember {
  #[wasm_bindgen(constructor)]
  pub fn new(
    inbox_id: String,
    account_addresses: Vec<String>,
    installation_ids: Vec<String>,
    permission_level: WasmPermissionLevel,
    consent_state: WasmConsentState,
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
pub struct WasmGroup {
  inner_client: Arc<RustXmtpClient>,
  group_id: Vec<u8>,
  created_at_ns: i64,
}

impl WasmGroup {
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

impl From<MlsGroup<RustXmtpClient>> for WasmGroup {
  fn from(mls_group: MlsGroup<RustXmtpClient>) -> Self {
    WasmGroup {
      inner_client: mls_group.client,
      group_id: mls_group.group_id,
      created_at_ns: mls_group.created_at_ns,
    }
  }
}

#[wasm_bindgen]
impl WasmGroup {
  #[wasm_bindgen]
  pub fn id(&self) -> String {
    hex::encode(self.group_id.clone())
  }

  #[wasm_bindgen]
  pub async fn send(&self, encoded_content: WasmEncodedContent) -> Result<String, JsError> {
    let encoded_content: EncodedContent = encoded_content.into();
    let group = self.to_mls_group();

    let message_id = group
      .send_message(encoded_content.encode_to_vec().as_slice())
      .await
      .map_err(|e| JsError::new(&format!("{e}")))?;

    Ok(hex::encode(message_id.clone()))
  }

  /// send a message without immediately publishing to the delivery service.
  #[wasm_bindgen]
  pub fn send_optimistic(&self, encoded_content: WasmEncodedContent) -> Result<String, JsError> {
    let encoded_content: EncodedContent = encoded_content.into();
    let group = self.to_mls_group();

    let id = group
      .send_message_optimistic(encoded_content.encode_to_vec().as_slice())
      .map_err(|e| JsError::new(&format!("{e}")))?;

    Ok(hex::encode(id.clone()))
  }

  /// Publish all unpublished messages
  #[wasm_bindgen]
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

  #[wasm_bindgen]
  pub fn find_messages(
    &self,
    opts: Option<WasmListMessagesOptions>,
  ) -> Result<Vec<WasmMessage>, JsError> {
    let opts = opts.unwrap_or_default();
    let group = self.to_mls_group();
    let messages: Vec<WasmMessage> = group
      .find_messages(&opts.into())
      .map_err(|e| JsError::new(&format!("{e}")))?
      .into_iter()
      .map(Into::into)
      .collect();

    Ok(messages)
  }

  #[wasm_bindgen]
  pub async fn list_members(&self) -> Result<JsValue, JsError> {
    let group = self.to_mls_group();
    let members: Vec<WasmGroupMember> = group
      .members()
      .await
      .map_err(|e| JsError::new(&format!("{e}")))?
      .into_iter()
      .map(|member| WasmGroupMember {
        inbox_id: member.inbox_id,
        account_addresses: member.account_addresses,
        installation_ids: member
          .installation_ids
          .into_iter()
          .map(|id| ed25519_public_key_to_address(id.as_slice()))
          .collect(),
        permission_level: match member.permission_level {
          PermissionLevel::Member => WasmPermissionLevel::Member,
          PermissionLevel::Admin => WasmPermissionLevel::Admin,
          PermissionLevel::SuperAdmin => WasmPermissionLevel::SuperAdmin,
        },
        consent_state: member.consent_state.into(),
      })
      .collect();

    Ok(serde_wasm_bindgen::to_value(&members)?)
  }

  #[wasm_bindgen]
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

  #[wasm_bindgen]
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

  #[wasm_bindgen]
  pub fn is_admin(&self, inbox_id: String) -> Result<bool, JsError> {
    let admin_list = self.admin_list()?;
    Ok(admin_list.contains(&inbox_id))
  }

  #[wasm_bindgen]
  pub fn is_super_admin(&self, inbox_id: String) -> Result<bool, JsError> {
    let super_admin_list = self.super_admin_list()?;
    Ok(super_admin_list.contains(&inbox_id))
  }

  #[wasm_bindgen]
  pub async fn add_members(&self, account_addresses: Vec<String>) -> Result<(), JsError> {
    let group = self.to_mls_group();

    group
      .add_members(&account_addresses)
      .await
      .map_err(|e| JsError::new(&format!("{e}")))?;

    Ok(())
  }

  #[wasm_bindgen]
  pub async fn add_admin(&self, inbox_id: String) -> Result<(), JsError> {
    let group = self.to_mls_group();
    group
      .update_admin_list(UpdateAdminListType::Add, inbox_id)
      .await
      .map_err(|e| JsError::new(&format!("{e}")))?;

    Ok(())
  }

  #[wasm_bindgen]
  pub async fn remove_admin(&self, inbox_id: String) -> Result<(), JsError> {
    let group = self.to_mls_group();

    group
      .update_admin_list(UpdateAdminListType::Remove, inbox_id)
      .await
      .map_err(|e| JsError::new(&format!("{e}")))?;

    Ok(())
  }

  #[wasm_bindgen]
  pub async fn add_super_admin(&self, inbox_id: String) -> Result<(), JsError> {
    let group = self.to_mls_group();

    group
      .update_admin_list(UpdateAdminListType::AddSuper, inbox_id)
      .await
      .map_err(|e| JsError::new(&format!("{e}")))?;

    Ok(())
  }

  #[wasm_bindgen]
  pub async fn remove_super_admin(&self, inbox_id: String) -> Result<(), JsError> {
    let group = self.to_mls_group();

    group
      .update_admin_list(UpdateAdminListType::RemoveSuper, inbox_id)
      .await
      .map_err(|e| JsError::new(&format!("{e}")))?;

    Ok(())
  }

  #[wasm_bindgen]
  pub fn group_permissions(&self) -> Result<WasmGroupPermissions, JsError> {
    let group = self.to_mls_group();

    let permissions = group
      .permissions()
      .map_err(|e| JsError::new(&format!("{e}")))?;

    Ok(WasmGroupPermissions::new(permissions))
  }

  #[wasm_bindgen]
  pub async fn add_members_by_inbox_id(&self, inbox_ids: Vec<String>) -> Result<(), JsError> {
    let group = self.to_mls_group();

    group
      .add_members_by_inbox_id(&inbox_ids)
      .await
      .map_err(|e| JsError::new(&format!("{e}")))?;

    Ok(())
  }

  #[wasm_bindgen]
  pub async fn remove_members(&self, account_addresses: Vec<String>) -> Result<(), JsError> {
    let group = self.to_mls_group();

    group
      .remove_members(&account_addresses)
      .await
      .map_err(|e| JsError::new(&format!("{e}")))?;

    Ok(())
  }

  #[wasm_bindgen]
  pub async fn remove_members_by_inbox_id(&self, inbox_ids: Vec<String>) -> Result<(), JsError> {
    let group = self.to_mls_group();

    group
      .remove_members_by_inbox_id(&inbox_ids)
      .await
      .map_err(|e| JsError::new(&format!("{e}")))?;

    Ok(())
  }

  #[wasm_bindgen]
  pub async fn update_group_name(&self, group_name: String) -> Result<(), JsError> {
    let group = self.to_mls_group();

    group
      .update_group_name(group_name)
      .await
      .map_err(|e| JsError::new(&format!("{e}")))?;

    Ok(())
  }

  #[wasm_bindgen]
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

  #[wasm_bindgen]
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

  #[wasm_bindgen]
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

  #[wasm_bindgen]
  pub async fn update_group_description(&self, group_description: String) -> Result<(), JsError> {
    let group = self.to_mls_group();

    group
      .update_group_description(group_description)
      .await
      .map_err(|e| JsError::new(&format!("{e}")))?;

    Ok(())
  }

  #[wasm_bindgen]
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

  #[wasm_bindgen]
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

  #[wasm_bindgen]
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

  #[wasm_bindgen]
  pub fn created_at_ns(&self) -> i64 {
    self.created_at_ns
  }

  #[wasm_bindgen]
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

  #[wasm_bindgen]
  pub fn added_by_inbox_id(&self) -> Result<String, JsError> {
    let group = self.to_mls_group();

    group
      .added_by_inbox_id()
      .map_err(|e| JsError::new(&format!("{e}")))
  }

  #[wasm_bindgen]
  pub fn group_metadata(&self) -> Result<WasmGroupMetadata, JsError> {
    let group = self.to_mls_group();
    let metadata = group
      .metadata(
        group
          .mls_provider()
          .map_err(|e| JsError::new(&format!("{e}")))?,
      )
      .map_err(|e| JsError::new(&format!("{e}")))?;

    Ok(WasmGroupMetadata { inner: metadata })
  }

  #[wasm_bindgen]
  pub fn dm_peer_inbox_id(&self) -> Result<String, JsError> {
    let group = self.to_mls_group();

    group
      .dm_inbox_id()
      .map_err(|e| JsError::new(&format!("{e}")))
  }
}
