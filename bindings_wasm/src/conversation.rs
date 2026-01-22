use crate::client::RustMlsGroup;
use crate::conversations::{ConversationDebugInfo, HmacKey, MessageDisappearingSettings};
use crate::encoded_content::EncodedContent;
use crate::identity::{Identifier, IdentityExt};
use crate::messages::{ListMessagesOptions, Message, MessageWithReactions};
use crate::permissions::{MetadataField, PermissionPolicy, PermissionUpdateType};
use crate::streams::{StreamCallback, StreamCloser};
use crate::{
  consent_state::ConsentState, enriched_message::DecodedMessage, permissions::GroupPermissions,
};
use std::collections::HashMap;
use wasm_bindgen::JsValue;
use wasm_bindgen::{JsError, prelude::wasm_bindgen};
use xmtp_db::group::{ConversationType, DmIdExt};
use xmtp_db::group_message::MsgQueryArgs;
use xmtp_mls::{
  groups::{
    MlsGroup, UpdateAdminListType, intents::PermissionUpdateType as XmtpPermissionUpdateType,
    members::PermissionLevel as XmtpPermissionLevel,
  },
  mls_common::{
    group_metadata::GroupMetadata as XmtpGroupMetadata,
    group_mutable_metadata::MetadataField as XmtpMetadataField,
  },
};
use xmtp_proto::xmtp::mls::message_contents::EncodedContent as XmtpEncodedContent;

use prost::Message as ProstMessage;

#[wasm_bindgen]
pub struct SendMessageOpts {
  #[wasm_bindgen(js_name = shouldPush)]
  pub should_push: bool,
}

#[wasm_bindgen]
impl SendMessageOpts {
  #[wasm_bindgen(constructor)]
  pub fn new(should_push: bool) -> Self {
    Self { should_push }
  }
}

impl From<SendMessageOpts> for xmtp_mls::groups::send_message_opts::SendMessageOpts {
  fn from(opts: SendMessageOpts) -> Self {
    xmtp_mls::groups::send_message_opts::SendMessageOpts {
      should_push: opts.should_push,
    }
  }
}

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
      ConversationType::Oneshot => "oneshot".to_string(),
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
  #[wasm_bindgen(js_name = accountIdentifiers)]
  #[serde(rename = "accountIdentifiers")]
  pub account_identifiers: Vec<Identifier>,
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
    #[wasm_bindgen(js_name = inboxId)] inbox_id: String,
    #[wasm_bindgen(js_name = accountIdentifiers)] account_identifiers: Vec<Identifier>,
    #[wasm_bindgen(js_name = installationIds)] installation_ids: Vec<String>,
    #[wasm_bindgen(js_name = permissionLevel)] permission_level: PermissionLevel,
    #[wasm_bindgen(js_name = consentState)] consent_state: ConsentState,
  ) -> Self {
    Self {
      inbox_id,
      account_identifiers,
      installation_ids,
      permission_level,
      consent_state,
    }
  }
}

#[wasm_bindgen]
#[derive(Clone)]
pub struct Conversation {
  inner_group: RustMlsGroup,
  group_id: Vec<u8>,
  dm_id: Option<String>,
  created_at_ns: i64,
}

impl Conversation {
  pub fn new(
    inner_group: RustMlsGroup,
    group_id: Vec<u8>,
    dm_id: Option<String>,
    created_at_ns: i64,
  ) -> Self {
    Self {
      inner_group,
      group_id,
      dm_id,
      created_at_ns,
    }
  }

  pub fn to_mls_group(&self) -> RustMlsGroup {
    MlsGroup::new(
      self.inner_group.context.clone(),
      self.group_id.clone(),
      self.dm_id.clone(),
      self.inner_group.conversation_type,
      self.created_at_ns,
    )
  }
}

impl From<RustMlsGroup> for Conversation {
  fn from(mls_group: RustMlsGroup) -> Self {
    Conversation {
      group_id: mls_group.group_id.clone(),
      dm_id: mls_group.dm_id.clone(),
      created_at_ns: mls_group.created_at_ns,
      inner_group: mls_group,
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
  pub async fn send(
    &self,
    encoded_content: EncodedContent,
    opts: SendMessageOpts,
  ) -> Result<String, JsError> {
    let encoded_content: XmtpEncodedContent = encoded_content.into();
    let group = self.to_mls_group();

    let message_id = group
      .send_message(encoded_content.encode_to_vec().as_slice(), opts.into())
      .await
      .map_err(crate::error)?;

    Ok(hex::encode(message_id.clone()))
  }

  /// send a message without immediately publishing to the delivery service.
  #[wasm_bindgen(js_name = sendOptimistic)]
  pub fn send_optimistic(
    &self,
    encoded_content: EncodedContent,
    opts: SendMessageOpts,
  ) -> Result<String, JsError> {
    let encoded_content: XmtpEncodedContent = encoded_content.into();
    let group = self.to_mls_group();

    let id = group
      .send_message_optimistic(encoded_content.encode_to_vec().as_slice(), opts.into())
      .map_err(crate::error)?;

    Ok(hex::encode(id.clone()))
  }

  /// Publish all unpublished messages
  #[wasm_bindgen(js_name = publishMessages)]
  pub async fn publish_messages(&self) -> Result<(), JsError> {
    let group = self.to_mls_group();
    group
      .publish_messages()
      .await
      .map_err(crate::error)?;

    Ok(())
  }

  #[wasm_bindgen]
  pub async fn sync(&self) -> Result<(), JsError> {
    let group = self.to_mls_group();

    group
      .sync()
      .await
      .map_err(crate::error)?;

    Ok(())
  }

  #[wasm_bindgen(js_name = findMessages)]
  pub async fn find_messages(
    &self,
    opts: Option<ListMessagesOptions>,
  ) -> Result<Vec<Message>, JsError> {
    let opts = opts.unwrap_or_default();
    let group = self.to_mls_group();
    let conversation_type = group
      .conversation_type()
      .await
      .map_err(crate::error)?;
    let kind = match conversation_type {
      ConversationType::Group => None,
      ConversationType::Dm => None,
      ConversationType::Sync => None,
      ConversationType::Oneshot => None,
    };

    let opts = MsgQueryArgs {
      kind,
      ..opts.into()
    };
    let messages: Vec<Message> = group
      .find_messages(&opts)
      .map_err(crate::error)?
      .into_iter()
      .map(Into::into)
      .collect();

    Ok(messages)
  }

  #[wasm_bindgen(js_name = countMessages)]
  pub async fn count_messages(&self, opts: Option<ListMessagesOptions>) -> Result<i64, JsError> {
    let opts = opts.unwrap_or_default();
    let group = self.to_mls_group();
    let query_args = opts.into();
    let count = group
      .count_messages(&query_args)
      .map_err(crate::error)?;

    Ok(count)
  }

  #[wasm_bindgen(js_name = findMessagesWithReactions)]
  pub async fn find_messages_with_reactions(
    &self,
    opts: Option<ListMessagesOptions>,
  ) -> Result<Vec<MessageWithReactions>, JsError> {
    let opts = opts.unwrap_or_default();
    let group = self.to_mls_group();
    let conversation_type = group
      .conversation_type()
      .await
      .map_err(crate::error)?;
    let kind = match conversation_type {
      ConversationType::Group => None,
      ConversationType::Dm => None,
      ConversationType::Sync => None,
      ConversationType::Oneshot => None,
    };

    let opts = MsgQueryArgs {
      kind,
      ..opts.into()
    };

    let messages: Vec<MessageWithReactions> = group
      .find_messages_with_reactions(&opts)?
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
      .map_err(crate::error)?
      .into_iter()
      .map(|member| GroupMember {
        inbox_id: member.inbox_id,
        account_identifiers: member
          .account_identifiers
          .iter()
          .cloned()
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

    Ok(crate::to_value(&members)?)
  }

  #[wasm_bindgen(js_name = adminList)]
  pub fn admin_list(&self) -> Result<Vec<String>, JsError> {
    let group = self.to_mls_group();
    let admin_list = group
      .admin_list()
      .map_err(crate::error)?;

    Ok(admin_list)
  }

  #[wasm_bindgen(js_name = superAdminList)]
  pub fn super_admin_list(&self) -> Result<Vec<String>, JsError> {
    let group = self.to_mls_group();
    let super_admin_list = group
      .super_admin_list()
      .map_err(crate::error)?;

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
  pub async fn add_members(&self, account_identifiers: Vec<Identifier>) -> Result<(), JsError> {
    let group = self.to_mls_group();

    group
      .add_members(&account_identifiers.to_internal()?)
      .await
      .map_err(crate::error)?;

    Ok(())
  }

  #[wasm_bindgen(js_name = addAdmin)]
  pub async fn add_admin(&self, inbox_id: String) -> Result<(), JsError> {
    let group = self.to_mls_group();
    group
      .update_admin_list(UpdateAdminListType::Add, inbox_id)
      .await
      .map_err(crate::error)?;

    Ok(())
  }

  #[wasm_bindgen(js_name = removeAdmin)]
  pub async fn remove_admin(&self, inbox_id: String) -> Result<(), JsError> {
    let group = self.to_mls_group();

    group
      .update_admin_list(UpdateAdminListType::Remove, inbox_id)
      .await
      .map_err(crate::error)?;

    Ok(())
  }

  #[wasm_bindgen(js_name = addSuperAdmin)]
  pub async fn add_super_admin(&self, inbox_id: String) -> Result<(), JsError> {
    let group = self.to_mls_group();

    group
      .update_admin_list(UpdateAdminListType::AddSuper, inbox_id)
      .await
      .map_err(crate::error)?;

    Ok(())
  }

  #[wasm_bindgen(js_name = removeSuperAdmin)]
  pub async fn remove_super_admin(&self, inbox_id: String) -> Result<(), JsError> {
    let group = self.to_mls_group();

    group
      .update_admin_list(UpdateAdminListType::RemoveSuper, inbox_id)
      .await
      .map_err(crate::error)?;

    Ok(())
  }

  #[wasm_bindgen(js_name = groupPermissions)]
  pub fn group_permissions(&self) -> Result<GroupPermissions, JsError> {
    let group = self.to_mls_group();

    let permissions = group
      .permissions()
      .map_err(crate::error)?;

    Ok(GroupPermissions::new(permissions))
  }

  #[wasm_bindgen(js_name = addMembersByInboxId)]
  pub async fn add_members_by_inbox_id(&self, inbox_ids: Vec<String>) -> Result<(), JsError> {
    let group = self.to_mls_group();

    group
      .add_members_by_inbox_id(&inbox_ids)
      .await
      .map_err(crate::error)?;

    Ok(())
  }

  #[wasm_bindgen(js_name = removeMembers)]
  pub async fn remove_members(&self, account_identifiers: Vec<Identifier>) -> Result<(), JsError> {
    let group = self.to_mls_group();

    group
      .remove_members(&account_identifiers.to_internal()?)
      .await
      .map_err(crate::error)?;

    Ok(())
  }

  #[wasm_bindgen(js_name = removeMembersByInboxId)]
  pub async fn remove_members_by_inbox_id(&self, inbox_ids: Vec<String>) -> Result<(), JsError> {
    let group = self.to_mls_group();

    let ids = inbox_ids.iter().map(AsRef::as_ref).collect::<Vec<&str>>();
    group
      .remove_members_by_inbox_id(ids.as_slice())
      .await
      .map_err(crate::error)?;

    Ok(())
  }

  #[wasm_bindgen(js_name = updateGroupName)]
  pub async fn update_group_name(&self, group_name: String) -> Result<(), JsError> {
    let group = self.to_mls_group();

    group
      .update_group_name(group_name)
      .await
      .map_err(crate::error)?;

    Ok(())
  }

  #[wasm_bindgen(js_name = groupName)]
  pub fn group_name(&self) -> Result<String, JsError> {
    let group = self.to_mls_group();

    let group_name = group
      .group_name()
      .map_err(crate::error)?;

    Ok(group_name)
  }

  #[wasm_bindgen(js_name = updateAppData)]
  pub async fn update_app_data(&self, app_data: String) -> Result<(), JsError> {
    let group = self.to_mls_group();

    group
      .update_app_data(app_data)
      .await
      .map_err(crate::error)?;

    Ok(())
  }

  #[wasm_bindgen(js_name = appData)]
  pub fn app_data(&self) -> Result<String, JsError> {
    let group = self.to_mls_group();

    let app_data = group
      .app_data()
      .map_err(crate::error)?;

    Ok(app_data)
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
      .map_err(crate::error)?;

    Ok(())
  }

  #[wasm_bindgen(js_name = groupImageUrlSquare)]
  pub fn group_image_url_square(&self) -> Result<String, JsError> {
    let group = self.to_mls_group();

    let group_image_url_square = group
      .group_image_url_square()
      .map_err(crate::error)?;

    Ok(group_image_url_square)
  }

  #[wasm_bindgen(js_name = updateGroupDescription)]
  pub async fn update_group_description(&self, group_description: String) -> Result<(), JsError> {
    let group = self.to_mls_group();

    group
      .update_group_description(group_description)
      .await
      .map_err(crate::error)?;

    Ok(())
  }

  #[wasm_bindgen(js_name = groupDescription)]
  pub fn group_description(&self) -> Result<String, JsError> {
    let group = self.to_mls_group();

    let group_description = group
      .group_description()
      .map_err(crate::error)?;

    Ok(group_description)
  }

  #[wasm_bindgen(js_name = stream)]
  pub fn stream(&self, callback: StreamCallback) -> Result<StreamCloser, JsError> {
    let on_close_cb = callback.clone();
    let stream_closer = MlsGroup::stream_with_callback(
      self.inner_group.context.clone(),
      self.group_id.clone(),
      move |message| match message {
        Ok(item) => callback.on_message(item.into()),
        Err(e) => callback.on_error(JsError::from(e)),
      },
      move || on_close_cb.on_close(),
    );

    Ok(StreamCloser::new(stream_closer))
  }

  #[wasm_bindgen(js_name = createdAtNs)]
  pub fn created_at_ns(&self) -> i64 {
    self.created_at_ns
  }

  #[wasm_bindgen(js_name = isActive)]
  pub fn is_active(&self) -> Result<bool, JsError> {
    let group = self.to_mls_group();

    group.is_active().map_err(|e| JsError::new(&format!("{e}")))
  }

  #[wasm_bindgen(js_name = pausedForVersion)]
  pub fn paused_for_version(&self) -> Result<Option<String>, JsError> {
    let group = self.to_mls_group();

    group
      .paused_for_version()
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
  pub async fn group_metadata(&self) -> Result<GroupMetadata, JsError> {
    let group = self.to_mls_group();
    let metadata = group
      .metadata()
      .await
      .map_err(crate::error)?;

    Ok(GroupMetadata { inner: metadata })
  }

  #[wasm_bindgen(js_name = dmPeerInboxId)]
  pub fn dm_peer_inbox_id(&self) -> Result<String, JsError> {
    let inbox_id = self.inner_group.context.inbox_id();

    Ok(
      self
        .to_mls_group()
        .dm_id
        .as_ref()
        .ok_or(JsError::new("Not a DM conversation or missing DM ID"))?
        .other_inbox_id(inbox_id),
    )
  }

  #[wasm_bindgen(js_name = updatePermissionPolicy)]
  pub async fn update_permission_policy(
    &self,
    permission_update_type: PermissionUpdateType,
    permission_policy_option: PermissionPolicy,
    metadata_field: Option<MetadataField>,
  ) -> Result<(), JsError> {
    self
      .to_mls_group()
      .update_permission_policy(
        XmtpPermissionUpdateType::from(&permission_update_type),
        permission_policy_option.try_into()?,
        metadata_field.map(|field| XmtpMetadataField::from(&field)),
      )
      .await
      .map_err(Into::into)
  }

  #[wasm_bindgen(js_name = updateMessageDisappearingSettings)]
  pub async fn update_message_disappearing_settings(
    &self,
    settings: MessageDisappearingSettings,
  ) -> Result<(), JsError> {
    self
      .to_mls_group()
      .update_conversation_message_disappearing_settings(settings.into())
      .await
      .map_err(crate::error)?;

    Ok(())
  }

  #[wasm_bindgen(js_name = removeMessageDisappearingSettings)]
  pub async fn remove_message_disappearing_settings(&self) -> Result<(), JsError> {
    self
      .to_mls_group()
      .remove_conversation_message_disappearing_settings()
      .await
      .map_err(crate::error)?;

    Ok(())
  }

  #[wasm_bindgen(js_name = messageDisappearingSettings)]
  pub fn message_disappearing_settings(
    &self,
  ) -> Result<Option<MessageDisappearingSettings>, JsError> {
    let settings = self
      .inner_group
      .disappearing_settings()
      .map_err(crate::error)?;

    match settings {
      Some(s) => Ok(Some(s.into())),
      None => Ok(None),
    }
  }

  #[wasm_bindgen(js_name = isMessageDisappearingEnabled)]
  pub fn is_message_disappearing_enabled(&self) -> Result<bool, JsError> {
    self.message_disappearing_settings().map(|settings| {
      settings
        .as_ref()
        .is_some_and(|s| s.from_ns > 0 && s.in_ns > 0)
    })
  }

  #[wasm_bindgen(js_name = getHmacKeys)]
  pub fn get_hmac_keys(&self) -> Result<JsValue, JsError> {
    let group = self.to_mls_group();

    let dms = self
      .inner_group
      .find_duplicate_dms()
      .map_err(|e| JsError::new(&e.to_string()))?;

    let mut hmac_map: HashMap<String, Vec<HmacKey>> = HashMap::new();
    for conversation in dms {
      let id = hex::encode(&conversation.group_id);
      let keys = conversation
        .hmac_keys(-1..=1)
        .map_err(|e| JsError::new(format!("{}", e).as_str()))?
        .into_iter()
        .map(Into::into)
        .collect::<Vec<_>>();
      hmac_map.insert(id, keys);
    }

    let keys = group
      .hmac_keys(-1..=1)
      .map_err(crate::error)?
      .into_iter()
      .map(Into::into)
      .collect::<Vec<HmacKey>>();

    hmac_map.insert(self.id(), keys);

    Ok(crate::to_value(&hmac_map)?)
  }

  #[wasm_bindgen(js_name = getDebugInfo)]
  pub async fn debug_info(&self) -> Result<JsValue, JsError> {
    let group = self.to_mls_group();
    let debug_info = group
      .debug_info()
      .await
      .map_err(crate::error)?;

    Ok(crate::to_value(&ConversationDebugInfo {
      epoch: debug_info.epoch,
      maybe_forked: debug_info.maybe_forked,
      fork_details: debug_info.fork_details,
      is_commit_log_forked: debug_info.is_commit_log_forked,
      local_commit_log: debug_info.local_commit_log,
      remote_commit_log: debug_info.remote_commit_log,
      cursor: debug_info.cursor.into_iter().map(Into::into).collect(),
    })?)
  }

  #[wasm_bindgen(js_name = findDuplicateDms)]
  pub async fn find_duplicate_dms(&self) -> Result<Vec<Conversation>, JsError> {
    // Await the async function first, then handle the error
    let dms = self
      .inner_group
      .find_duplicate_dms()
      .map_err(|e| JsError::new(&e.to_string()))?;

    let conversations: Vec<Conversation> = dms.into_iter().map(Into::into).collect();

    Ok(conversations)
  }

  #[wasm_bindgen(js_name = findMessagesV2)]
  pub async fn enriched_messages(
    &self,
    opts: Option<ListMessagesOptions>,
  ) -> Result<Vec<DecodedMessage>, JsError> {
    let opts = opts.unwrap_or_default();
    let group = self.to_mls_group();
    let messages: Vec<DecodedMessage> = group
      .find_messages_v2(&opts.into())
      .map_err(crate::error)?
      .into_iter()
      .map(|msg| msg.into())
      .collect();

    Ok(messages)
  }

  #[wasm_bindgen(js_name = getLastReadTimes)]
  pub async fn get_last_read_times(&self) -> Result<JsValue, JsError> {
    let group = self.to_mls_group();
    let times = group
      .get_last_read_times()
      .map_err(crate::error)?;

    Ok(crate::to_value(&times)?)
  }
}

#[cfg(test)]
mod tests {
  use wasm_bindgen_test::wasm_bindgen_test;
  use xmtp_db::group_message::{ContentType, DeliveryStatus, GroupMessageKind, StoredGroupMessage};
  wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

  #[wasm_bindgen_test]
  fn test_group_message_to_object() {
    let stored_message = StoredGroupMessage {
      id: xmtp_common::rand_vec::<32>(),
      group_id: xmtp_common::rand_vec::<32>(),
      decrypted_message_bytes: xmtp_common::rand_vec::<32>(),
      sent_at_ns: 1738354508964432000,
      inserted_at_ns: 1738354508964432000,
      kind: GroupMessageKind::Application,
      sender_installation_id: xmtp_common::rand_vec::<32>(),
      sender_inbox_id: String::from("test"),
      delivery_status: DeliveryStatus::Published,
      content_type: ContentType::Text,
      version_major: 4,
      version_minor: 123,
      authority_id: String::from("test"),
      reference_id: None,
      originator_id: 0,
      sequence_id: 0,
      expire_at_ns: None,
    };
    crate::to_value(&stored_message).unwrap();
  }
}
