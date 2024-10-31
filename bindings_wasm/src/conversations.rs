use std::sync::Arc;
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::{JsError, JsValue};
use xmtp_mls::groups::group_metadata::ConversationType;
use xmtp_mls::groups::{GroupMetadataOptions, PreconfiguredPolicies};
use xmtp_mls::storage::group::GroupMembershipState;
use xmtp_mls::storage::group::GroupQueryArgs;

use crate::messages::WasmMessage;
use crate::permissions::WasmGroupPermissionsOptions;
use crate::{groups::WasmGroup, mls_client::RustXmtpClient};

#[wasm_bindgen]
#[derive(Debug, Clone)]
pub enum WasmConversationType {
  Dm = 0,
  Group = 1,
  Sync = 2,
}

impl From<ConversationType> for WasmConversationType {
  fn from(ct: ConversationType) -> Self {
    match ct {
      ConversationType::Dm => WasmConversationType::Dm,
      ConversationType::Group => WasmConversationType::Group,
      ConversationType::Sync => WasmConversationType::Sync,
    }
  }
}

impl From<WasmConversationType> for ConversationType {
  fn from(nct: WasmConversationType) -> Self {
    match nct {
      WasmConversationType::Dm => ConversationType::Dm,
      WasmConversationType::Group => ConversationType::Group,
      WasmConversationType::Sync => ConversationType::Sync,
    }
  }
}

#[wasm_bindgen]
#[derive(Debug, Clone)]
pub enum WasmGroupMembershipState {
  Allowed = 0,
  Rejected = 1,
  Pending = 2,
}

impl From<GroupMembershipState> for WasmGroupMembershipState {
  fn from(gms: GroupMembershipState) -> Self {
    match gms {
      GroupMembershipState::Allowed => WasmGroupMembershipState::Allowed,
      GroupMembershipState::Rejected => WasmGroupMembershipState::Rejected,
      GroupMembershipState::Pending => WasmGroupMembershipState::Pending,
    }
  }
}

impl From<WasmGroupMembershipState> for GroupMembershipState {
  fn from(ngms: WasmGroupMembershipState) -> Self {
    match ngms {
      WasmGroupMembershipState::Allowed => GroupMembershipState::Allowed,
      WasmGroupMembershipState::Rejected => GroupMembershipState::Rejected,
      WasmGroupMembershipState::Pending => GroupMembershipState::Pending,
    }
  }
}

#[wasm_bindgen(getter_with_clone)]
#[derive(Default)]
pub struct WasmListConversationsOptions {
  pub allowed_states: Option<Vec<WasmGroupMembershipState>>,
  pub conversation_type: Option<WasmConversationType>,
  pub created_after_ns: Option<i64>,
  pub created_before_ns: Option<i64>,
  pub limit: Option<i64>,
}

impl From<WasmListConversationsOptions> for GroupQueryArgs {
  fn from(opts: WasmListConversationsOptions) -> GroupQueryArgs {
    GroupQueryArgs::default()
      .maybe_allowed_states(
        opts
          .allowed_states
          .map(|states| states.into_iter().map(From::from).collect()),
      )
      .maybe_conversation_type(opts.conversation_type.map(Into::into))
      .maybe_created_after_ns(opts.created_after_ns)
      .maybe_created_before_ns(opts.created_before_ns)
      .maybe_limit(opts.limit)
  }
}

#[wasm_bindgen]
impl WasmListConversationsOptions {
  #[wasm_bindgen(constructor)]
  pub fn new(
    allowed_states: Option<Vec<WasmGroupMembershipState>>,
    conversation_type: Option<WasmConversationType>,
    created_after_ns: Option<i64>,
    created_before_ns: Option<i64>,
    limit: Option<i64>,
  ) -> Self {
    Self {
      allowed_states,
      conversation_type,
      created_after_ns,
      created_before_ns,
      limit,
    }
  }
}

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone)]
pub struct WasmCreateGroupOptions {
  pub permissions: Option<WasmGroupPermissionsOptions>,
  pub group_name: Option<String>,
  pub group_image_url_square: Option<String>,
  pub group_description: Option<String>,
  pub group_pinned_frame_url: Option<String>,
}

#[wasm_bindgen]
impl WasmCreateGroupOptions {
  #[wasm_bindgen(constructor)]
  pub fn new(
    permissions: Option<WasmGroupPermissionsOptions>,
    group_name: Option<String>,
    group_image_url_square: Option<String>,
    group_description: Option<String>,
    group_pinned_frame_url: Option<String>,
  ) -> Self {
    Self {
      permissions,
      group_name,
      group_image_url_square,
      group_description,
      group_pinned_frame_url,
    }
  }
}

impl WasmCreateGroupOptions {
  pub fn into_group_metadata_options(self) -> GroupMetadataOptions {
    GroupMetadataOptions {
      name: self.group_name,
      image_url_square: self.group_image_url_square,
      description: self.group_description,
      pinned_frame_url: self.group_pinned_frame_url,
    }
  }
}

#[wasm_bindgen]
pub struct WasmConversations {
  inner_client: Arc<RustXmtpClient>,
}

impl WasmConversations {
  pub fn new(inner_client: Arc<RustXmtpClient>) -> Self {
    Self { inner_client }
  }
}

#[wasm_bindgen]
impl WasmConversations {
  #[wasm_bindgen]
  pub async fn create_group(
    &self,
    account_addresses: Vec<String>,
    options: Option<WasmCreateGroupOptions>,
  ) -> Result<WasmGroup, JsError> {
    let options = match options {
      Some(options) => options,
      None => WasmCreateGroupOptions {
        permissions: None,
        group_name: None,
        group_image_url_square: None,
        group_description: None,
        group_pinned_frame_url: None,
      },
    };

    let group_permissions = match options.permissions {
      Some(WasmGroupPermissionsOptions::AllMembers) => {
        Some(PreconfiguredPolicies::AllMembers.to_policy_set())
      }
      Some(WasmGroupPermissionsOptions::AdminOnly) => {
        Some(PreconfiguredPolicies::AdminsOnly.to_policy_set())
      }
      _ => None,
    };

    let metadata_options = options.clone().into_group_metadata_options();

    let convo = if account_addresses.is_empty() {
      self
        .inner_client
        .create_group(group_permissions, metadata_options)
        .map_err(|e| JsError::new(format!("{}", e).as_str()))?
    } else {
      self
        .inner_client
        .create_group_with_members(&account_addresses, group_permissions, metadata_options)
        .await
        .map_err(|e| JsError::new(format!("{}", e).as_str()))?
    };

    Ok(convo.into())
  }

  #[wasm_bindgen]
  pub async fn create_dm(&self, account_address: String) -> Result<WasmGroup, JsError> {
    let convo = self
      .inner_client
      .create_dm(account_address)
      .await
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;

    Ok(convo.into())
  }

  #[wasm_bindgen]
  pub fn find_group_by_id(&self, group_id: String) -> Result<WasmGroup, JsError> {
    let group_id = hex::decode(group_id).map_err(|e| JsError::new(format!("{}", e).as_str()))?;

    let group = self
      .inner_client
      .group(group_id)
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;

    Ok(group.into())
  }

  #[wasm_bindgen]
  pub fn find_dm_by_target_inbox_id(&self, target_inbox_id: String) -> Result<WasmGroup, JsError> {
    let convo = self
      .inner_client
      .dm_group_from_target_inbox(target_inbox_id)
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;

    Ok(convo.into())
  }

  #[wasm_bindgen]
  pub fn find_message_by_id(&self, message_id: String) -> Result<WasmMessage, JsError> {
    let message_id =
      hex::decode(message_id).map_err(|e| JsError::new(format!("{}", e).as_str()))?;

    let message = self
      .inner_client
      .message(message_id)
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;

    Ok(message.into())
  }

  #[wasm_bindgen]
  pub async fn sync(&self) -> Result<(), JsError> {
    let conn = self
      .inner_client
      .store()
      .conn()
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;
    self
      .inner_client
      .sync_welcomes(&conn)
      .await
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;

    Ok(())
  }

  #[wasm_bindgen]
  pub async fn list(
    &self,
    opts: Option<WasmListConversationsOptions>,
  ) -> Result<js_sys::Array, JsError> {
    let convo_list: js_sys::Array = self
      .inner_client
      .find_groups(opts.unwrap_or_default().into())
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?
      .into_iter()
      .map(|group| {
        JsValue::from(WasmGroup::new(
          self.inner_client.clone(),
          group.group_id,
          group.created_at_ns,
        ))
      })
      .collect();

    Ok(convo_list)
  }

  #[wasm_bindgen]
  pub async fn list_groups(
    &self,
    opts: Option<WasmListConversationsOptions>,
  ) -> Result<js_sys::Array, JsError> {
    self
      .list(Some(WasmListConversationsOptions {
        conversation_type: Some(WasmConversationType::Group),
        ..opts.unwrap_or_default()
      }))
      .await
  }

  #[wasm_bindgen]
  pub async fn list_dms(
    &self,
    opts: Option<WasmListConversationsOptions>,
  ) -> Result<js_sys::Array, JsError> {
    self
      .list(Some(WasmListConversationsOptions {
        conversation_type: Some(WasmConversationType::Dm),
        ..opts.unwrap_or_default()
      }))
      .await
  }
}
