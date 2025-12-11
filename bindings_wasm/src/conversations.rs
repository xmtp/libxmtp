use crate::consent_state::{Consent, ConsentState};
use crate::enriched_message::DecodedMessage;
use crate::identity::Identifier;
use crate::messages::Message;
use crate::permissions::{GroupPermissionsOptions, PermissionPolicySet};
use crate::streams::{ConversationStream, StreamCallback, StreamCloser};
use crate::user_preferences::UserPreference;
use crate::{client::RustXmtpClient, conversation::Conversation};
use std::collections::HashMap;
use std::sync::Arc;
use wasm_bindgen::UnwrapThrowExt;
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::{JsError, JsValue};
use wasm_streams::ReadableStream;
use xmtp_db::consent_record::ConsentState as XmtpConsentState;
use xmtp_db::group::GroupMembershipState as XmtpGroupMembershipState;
use xmtp_db::group::GroupQueryArgs;
use xmtp_db::group::{ConversationType as XmtpConversationType, GroupQueryOrderBy};
use xmtp_db::user_preferences::HmacKey as XmtpHmacKey;
use xmtp_mls::groups::PreconfiguredPolicies;
use xmtp_mls::mls_common::group::{DMMetadataOptions, GroupMetadataOptions};
use xmtp_mls::mls_common::group_mutable_metadata::MessageDisappearingSettings as XmtpMessageDisappearingSettings;
use xmtp_proto::types::Cursor;

#[wasm_bindgen]
#[derive(Debug, Clone)]
pub enum ConversationType {
  Dm = 0,
  Group = 1,
  Sync = 2,
  Oneshot = 3,
}

impl From<XmtpConversationType> for ConversationType {
  fn from(ct: XmtpConversationType) -> Self {
    match ct {
      XmtpConversationType::Dm => ConversationType::Dm,
      XmtpConversationType::Group => ConversationType::Group,
      XmtpConversationType::Sync => ConversationType::Sync,
      XmtpConversationType::Oneshot => ConversationType::Oneshot,
    }
  }
}

impl From<ConversationType> for XmtpConversationType {
  fn from(nct: ConversationType) -> Self {
    match nct {
      ConversationType::Dm => XmtpConversationType::Dm,
      ConversationType::Group => XmtpConversationType::Group,
      ConversationType::Sync => XmtpConversationType::Sync,
      ConversationType::Oneshot => XmtpConversationType::Oneshot,
    }
  }
}

#[wasm_bindgen]
#[derive(Debug, Clone)]
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
  fn from(ngms: GroupMembershipState) -> Self {
    match ngms {
      GroupMembershipState::Allowed => XmtpGroupMembershipState::Allowed,
      GroupMembershipState::Rejected => XmtpGroupMembershipState::Rejected,
      GroupMembershipState::Pending => XmtpGroupMembershipState::Pending,
      GroupMembershipState::Restored => XmtpGroupMembershipState::Restored,
      GroupMembershipState::PendingRemove => XmtpGroupMembershipState::PendingRemove,
    }
  }
}

#[wasm_bindgen]
#[derive(Debug, Clone)]
pub enum ListConversationsOrderBy {
  CreatedAt,
  LastActivity,
}

impl From<ListConversationsOrderBy> for GroupQueryOrderBy {
  fn from(order_by: ListConversationsOrderBy) -> Self {
    match order_by {
      ListConversationsOrderBy::CreatedAt => GroupQueryOrderBy::CreatedAt,
      ListConversationsOrderBy::LastActivity => GroupQueryOrderBy::LastActivity,
    }
  }
}

#[wasm_bindgen(getter_with_clone)]
#[derive(Default)]
pub struct ListConversationsOptions {
  #[wasm_bindgen(js_name = consentStates)]
  pub consent_states: Option<Vec<ConsentState>>,
  #[wasm_bindgen(js_name = conversationType)]
  pub conversation_type: Option<ConversationType>,
  #[wasm_bindgen(js_name = createdAfterNs)]
  pub created_after_ns: Option<i64>,
  #[wasm_bindgen(js_name = createdBeforeNs)]
  pub created_before_ns: Option<i64>,
  #[wasm_bindgen(js_name = includeDuplicateDms)]
  pub include_duplicate_dms: Option<bool>,
  #[wasm_bindgen(js_name = orderBy)]
  pub order_by: Option<ListConversationsOrderBy>,
  pub limit: Option<i64>,
}

impl From<ListConversationsOptions> for GroupQueryArgs {
  fn from(opts: ListConversationsOptions) -> GroupQueryArgs {
    GroupQueryArgs {
      consent_states: opts
        .consent_states
        .map(|states| states.into_iter().map(From::from).collect()),
      created_after_ns: opts.created_after_ns,
      created_before_ns: opts.created_before_ns,
      include_duplicate_dms: opts.include_duplicate_dms.unwrap_or_default(),
      limit: opts.limit,
      allowed_states: None,
      conversation_type: opts.conversation_type.map(Into::into),
      include_sync_groups: false,
      last_activity_before_ns: None,
      last_activity_after_ns: None,
      should_publish_commit_log: None,
      order_by: opts.order_by.map(Into::into),
    }
  }
}

#[wasm_bindgen]
impl ListConversationsOptions {
  #[wasm_bindgen(constructor)]
  pub fn new(
    consent_states: Option<Vec<ConsentState>>,
    conversation_type: Option<ConversationType>,
    created_after_ns: Option<i64>,
    created_before_ns: Option<i64>,
    include_duplicate_dms: Option<bool>,
    limit: Option<i64>,
    order_by: Option<ListConversationsOrderBy>,
  ) -> Self {
    Self {
      consent_states,
      conversation_type,
      created_after_ns,
      created_before_ns,
      include_duplicate_dms,
      limit,
      order_by,
    }
  }
}

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone)]
pub struct MessageDisappearingSettings {
  #[wasm_bindgen(js_name = fromNs)]
  pub from_ns: i64,
  #[wasm_bindgen(js_name = inNs)]
  pub in_ns: i64,
}

impl From<MessageDisappearingSettings> for XmtpMessageDisappearingSettings {
  fn from(value: MessageDisappearingSettings) -> Self {
    Self {
      from_ns: value.from_ns,
      in_ns: value.in_ns,
    }
  }
}

impl From<XmtpMessageDisappearingSettings> for MessageDisappearingSettings {
  fn from(value: XmtpMessageDisappearingSettings) -> Self {
    Self {
      from_ns: value.from_ns,
      in_ns: value.in_ns,
    }
  }
}

#[wasm_bindgen]
impl MessageDisappearingSettings {
  #[wasm_bindgen(constructor)]
  pub fn new(from_ns: i64, in_ns: i64) -> Self {
    Self { from_ns, in_ns }
  }
}

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone, serde::Serialize)]
pub struct XmtpCursor {
  pub originator_id: u32,
  // wasm doesn't support u64
  pub sequence_id: i64,
}

impl From<Cursor> for XmtpCursor {
  fn from(value: Cursor) -> Self {
    XmtpCursor {
      originator_id: value.originator_id,
      sequence_id: value.sequence_id as i64,
    }
  }
}

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone, serde::Serialize)]
pub struct ConversationDebugInfo {
  pub epoch: u64,
  #[wasm_bindgen(js_name = maybeForked)]
  #[serde(rename = "maybeForked")]
  pub maybe_forked: bool,
  #[wasm_bindgen(js_name = forkDetails)]
  #[serde(rename = "forkDetails")]
  pub fork_details: String,
  #[wasm_bindgen(js_name = isCommitLogForked)]
  #[serde(rename = "isCommitLogForked")]
  pub is_commit_log_forked: Option<bool>,
  #[wasm_bindgen(js_name = localCommitLog)]
  #[serde(rename = "localCommitLog")]
  pub local_commit_log: String,
  #[wasm_bindgen(js_name = remoteCommitLog)]
  #[serde(rename = "remoteCommitLog")]
  pub remote_commit_log: String,
  #[wasm_bindgen(js_name = cursor)]
  #[serde(rename = "cursor")]
  pub cursor: Vec<XmtpCursor>,
}

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone)]
pub struct CreateGroupOptions {
  pub permissions: Option<GroupPermissionsOptions>,
  #[wasm_bindgen(js_name = groupName)]
  pub group_name: Option<String>,
  #[wasm_bindgen(js_name = groupImageUrlSquare)]
  pub group_image_url_square: Option<String>,
  #[wasm_bindgen(js_name = groupDescription)]
  pub group_description: Option<String>,
  #[wasm_bindgen(js_name = customPermissionPolicySet)]
  pub custom_permission_policy_set: Option<PermissionPolicySet>,
  #[wasm_bindgen(js_name = messageDisappearingSettings)]
  pub message_disappearing_settings: Option<MessageDisappearingSettings>,
  #[wasm_bindgen(js_name = appData)]
  pub app_data: Option<String>,
}

#[wasm_bindgen]
impl CreateGroupOptions {
  #[wasm_bindgen(constructor)]
  #[allow(clippy::too_many_arguments)]
  pub fn new(
    permissions: Option<GroupPermissionsOptions>,
    group_name: Option<String>,
    group_image_url_square: Option<String>,
    group_description: Option<String>,
    custom_permission_policy_set: Option<PermissionPolicySet>,
    message_disappearing_settings: Option<MessageDisappearingSettings>,
    app_data: Option<String>,
  ) -> Self {
    Self {
      permissions,
      group_name,
      group_image_url_square,
      group_description,
      custom_permission_policy_set,
      message_disappearing_settings,
      app_data,
    }
  }
}

impl CreateGroupOptions {
  pub fn into_group_metadata_options(self) -> GroupMetadataOptions {
    GroupMetadataOptions {
      name: self.group_name,
      image_url_square: self.group_image_url_square,
      description: self.group_description,
      message_disappearing_settings: self
        .message_disappearing_settings
        .map(|settings| settings.into()),
      app_data: self.app_data,
    }
  }
}

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone, Default)]
pub struct CreateDMOptions {
  #[wasm_bindgen(js_name = messageDisappearingSettings)]
  pub message_disappearing_settings: Option<MessageDisappearingSettings>,
}

#[wasm_bindgen]
impl CreateDMOptions {
  #[wasm_bindgen(constructor)]
  #[allow(clippy::too_many_arguments)]
  pub fn new(message_disappearing_settings: Option<MessageDisappearingSettings>) -> Self {
    Self {
      message_disappearing_settings,
    }
  }
}

impl CreateDMOptions {
  pub fn into_dm_metadata_options(self) -> DMMetadataOptions {
    DMMetadataOptions {
      message_disappearing_settings: self
        .message_disappearing_settings
        .map(|settings| settings.into()),
    }
  }
}

#[wasm_bindgen(getter_with_clone)]
#[derive(serde::Serialize)]
pub struct HmacKey {
  pub key: Vec<u8>,
  pub epoch: i64,
}

impl From<XmtpHmacKey> for HmacKey {
  fn from(value: XmtpHmacKey) -> Self {
    Self {
      epoch: value.epoch,
      key: value.key.to_vec(),
    }
  }
}

#[wasm_bindgen(getter_with_clone)]
pub struct ConversationListItem {
  pub conversation: Conversation,
  #[wasm_bindgen(js_name = lastMessage)]
  pub last_message: Option<Message>,
  #[wasm_bindgen(js_name = isCommitLogForked)]
  pub is_commit_log_forked: Option<bool>,
}

#[wasm_bindgen]
impl ConversationListItem {
  #[wasm_bindgen(constructor)]
  pub fn new(
    conversation: Conversation,
    last_message: Option<Message>,
    is_commit_log_forked: Option<bool>,
  ) -> Self {
    Self {
      conversation,
      last_message,
      is_commit_log_forked,
    }
  }
}

#[wasm_bindgen]
pub struct Conversations {
  inner_client: Arc<RustXmtpClient>,
}

impl Conversations {
  pub fn new(inner_client: Arc<RustXmtpClient>) -> Self {
    Self { inner_client }
  }
}

#[wasm_bindgen]
impl Conversations {
  #[wasm_bindgen(js_name = createGroupOptimistic)]
  pub fn create_group_optimistic(
    &self,
    options: Option<CreateGroupOptions>,
  ) -> Result<Conversation, JsError> {
    let options = options.unwrap_or(CreateGroupOptions {
      permissions: None,
      group_name: None,
      group_image_url_square: None,
      group_description: None,
      custom_permission_policy_set: None,
      message_disappearing_settings: None,
      app_data: None,
    });

    if let Some(GroupPermissionsOptions::CustomPolicy) = options.permissions {
      if options.custom_permission_policy_set.is_none() {
        return Err(JsError::new("CustomPolicy must include policy set"));
      }
    } else if options.custom_permission_policy_set.is_some() {
      return Err(JsError::new("Only CustomPolicy may specify a policy set"));
    }

    let metadata_options = options.clone().into_group_metadata_options();

    let group_permissions = match options.permissions {
      Some(GroupPermissionsOptions::Default) => {
        Some(PreconfiguredPolicies::Default.to_policy_set())
      }
      Some(GroupPermissionsOptions::AdminOnly) => {
        Some(PreconfiguredPolicies::AdminsOnly.to_policy_set())
      }
      Some(GroupPermissionsOptions::CustomPolicy) => {
        if let Some(policy_set) = options.custom_permission_policy_set {
          Some(
            policy_set
              .try_into()
              .map_err(|e| JsError::new(format!("{}", e).as_str()))?,
          )
        } else {
          None
        }
      }
      _ => None,
    };

    let group = self
      .inner_client
      .create_group(group_permissions, Some(metadata_options))
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;

    Ok(group.into())
  }

  #[wasm_bindgen(js_name = createGroup)]
  pub async fn create_group(
    &self,
    account_identifiers: Vec<Identifier>,
    options: Option<CreateGroupOptions>,
  ) -> Result<Conversation, JsError> {
    let convo = self.create_group_optimistic(options)?;

    if !account_identifiers.is_empty() {
      convo.add_members(account_identifiers).await?;
    } else {
      convo.sync().await?;
    };

    Ok(convo)
  }

  #[wasm_bindgen(js_name = createGroupByInboxIds)]
  pub async fn create_group_by_inbox_ids(
    &self,
    inbox_ids: Vec<String>,
    options: Option<CreateGroupOptions>,
  ) -> Result<Conversation, JsError> {
    let convo = self.create_group_optimistic(options)?;

    if !inbox_ids.is_empty() {
      convo.add_members_by_inbox_id(inbox_ids).await?;
    } else {
      convo.sync().await?;
    };

    Ok(convo)
  }

  #[wasm_bindgen(js_name = createDm)]
  pub async fn find_or_create_dm(
    &self,
    account_identifier: Identifier,
    options: Option<CreateDMOptions>,
  ) -> Result<Conversation, JsError> {
    let convo = self
      .inner_client
      .find_or_create_dm(
        account_identifier.try_into()?,
        options.map(|opt| opt.into_dm_metadata_options()),
      )
      .await
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;

    Ok(convo.into())
  }

  #[wasm_bindgen(js_name = createDmByInboxId)]
  pub async fn find_or_create_dm_by_inbox_id(
    &self,
    inbox_id: String,
    options: Option<CreateDMOptions>,
  ) -> Result<Conversation, JsError> {
    let convo = self
      .inner_client
      .find_or_create_dm_by_inbox_id(inbox_id, options.map(|opt| opt.into_dm_metadata_options()))
      .await
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;

    Ok(convo.into())
  }

  #[wasm_bindgen(js_name = findGroupById)]
  pub fn find_group_by_id(&self, group_id: String) -> Result<Conversation, JsError> {
    let group_id = hex::decode(group_id).map_err(|e| JsError::new(format!("{}", e).as_str()))?;

    let group = self
      .inner_client
      .stitched_group(&group_id)
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;

    Ok(group.into())
  }

  #[wasm_bindgen(js_name = findDmByTargetInboxId)]
  pub fn find_dm_by_target_inbox_id(
    &self,
    target_inbox_id: String,
  ) -> Result<Conversation, JsError> {
    let convo = self
      .inner_client
      .dm_group_from_target_inbox(target_inbox_id)
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;

    Ok(convo.into())
  }

  #[wasm_bindgen(js_name = findMessageById)]
  pub fn find_message_by_id(&self, message_id: String) -> Result<Message, JsError> {
    let message_id =
      hex::decode(message_id).map_err(|e| JsError::new(format!("{}", e).as_str()))?;

    let message = self
      .inner_client
      .message(message_id)
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;

    Ok(message.into())
  }

  #[wasm_bindgen(js_name = findEnrichedMessageById)]
  pub async fn find_enriched_message_by_id(
    &self,
    message_id: String,
  ) -> Result<DecodedMessage, JsError> {
    let message_id =
      hex::decode(message_id).map_err(|e| JsError::new(format!("{}", e).as_str()))?;

    let message = self
      .inner_client
      .message_v2(message_id)
      .map_err(|e| JsError::new(&e.to_string()))?;

    Ok(message.into())
  }

  #[wasm_bindgen]
  pub async fn sync(&self) -> Result<(), JsError> {
    self
      .inner_client
      .sync_welcomes()
      .await
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;

    Ok(())
  }

  #[wasm_bindgen(js_name = syncAllConversations)]
  pub async fn sync_all_conversations(
    &self,
    consent_states: Option<Vec<ConsentState>>,
  ) -> Result<crate::client::GroupSyncSummary, JsError> {
    let consents: Option<Vec<XmtpConsentState>> =
      consent_states.map(|states| states.into_iter().map(|state| state.into()).collect());

    let summary = self
      .inner_client
      .sync_all_welcomes_and_groups(consents)
      .await
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;

    Ok(summary.into())
  }

  #[wasm_bindgen]
  pub fn list(&self, opts: Option<ListConversationsOptions>) -> Result<js_sys::Array, JsError> {
    let convo_list: js_sys::Array = self
      .inner_client
      .list_conversations(opts.unwrap_or_default().into())
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?
      .into_iter()
      .map(|group| {
        JsValue::from(ConversationListItem::new(
          group.group.into(),
          group.last_message.map(|m| m.into()),
          group.is_commit_log_forked,
        ))
      })
      .collect();

    Ok(convo_list)
  }

  #[wasm_bindgen(js_name = getHmacKeys)]
  pub fn get_hmac_keys(&self) -> Result<JsValue, JsError> {
    let inner = self.inner_client.as_ref();
    let conversations = inner
      .find_groups(GroupQueryArgs {
        include_duplicate_dms: true,
        ..Default::default()
      })
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;

    let mut hmac_map: HashMap<String, Vec<HmacKey>> = HashMap::new();
    for conversation in conversations {
      let id = hex::encode(&conversation.group_id);
      let keys = conversation
        .hmac_keys(-1..=1)
        .map_err(|e| JsError::new(format!("{}", e).as_str()))?
        .into_iter()
        .map(Into::into)
        .collect::<Vec<_>>();
      hmac_map.insert(id, keys);
    }

    Ok(crate::to_value(&hmac_map)?)
  }

  /// Returns a 'ReadableStream' of Conversations
  #[wasm_bindgen(js_name = streamLocal)]
  pub async fn stream_conversations_local(
    &self,
    conversation_type: Option<ConversationType>,
  ) -> Result<web_sys::ReadableStream, JsError> {
    let stream = self
      .inner_client
      .stream_conversations_owned(conversation_type.map(Into::into), false)
      .await?;
    let stream = ConversationStream::new(stream);
    Ok(ReadableStream::from_stream(stream).into_raw())
  }

  #[wasm_bindgen(js_name = stream)]
  pub fn stream(
    &self,
    callback: StreamCallback,
    conversation_type: Option<ConversationType>,
  ) -> Result<StreamCloser, JsError> {
    let on_close_cb = callback.clone();
    let stream_closer = RustXmtpClient::stream_conversations_with_callback(
      self.inner_client.clone(),
      conversation_type.map(Into::into),
      move |message| match message {
        Ok(item) => callback.on_conversation(item.into()),
        Err(e) => callback.on_error(JsError::from(e)),
      },
      move || on_close_cb.on_close(),
      false,
    );

    Ok(StreamCloser::new(stream_closer))
  }

  #[wasm_bindgen(js_name = "streamAllMessages")]
  pub fn stream_all_messages(
    &self,
    callback: StreamCallback,
    conversation_type: Option<ConversationType>,
    consent_states: Option<Vec<ConsentState>>,
  ) -> Result<StreamCloser, JsError> {
    let consents: Option<Vec<XmtpConsentState>> =
      consent_states.map(|states| states.into_iter().map(|state| state.into()).collect());

    let on_close_cb = callback.clone();
    let stream_closer = RustXmtpClient::stream_all_messages_with_callback(
      self.inner_client.context.clone(),
      conversation_type.map(Into::into),
      consents,
      move |message| match message {
        Ok(m) => callback.on_message(m.into()),
        Err(e) => callback.on_error(JsError::from(e)),
      },
      move || on_close_cb.on_close(),
    );
    Ok(StreamCloser::new(stream_closer))
  }

  #[wasm_bindgen(js_name = "streamConsent")]
  pub fn stream_consent(&self, callback: StreamCallback) -> Result<StreamCloser, JsError> {
    let on_close_cb = callback.clone();
    let stream_closer = RustXmtpClient::stream_consent_with_callback(
      self.inner_client.clone(),
      move |message| match message {
        Ok(m) => {
          let array = m.into_iter().map(Consent::from).collect::<Vec<Consent>>();
          let value = serde_wasm_bindgen::to_value(&array).unwrap_throw();
          callback.on_consent_update(value)
        }
        Err(e) => callback.on_error(JsError::from(e)),
      },
      move || on_close_cb.on_close(),
    );
    Ok(StreamCloser::new(stream_closer))
  }

  #[wasm_bindgen(js_name = "streamPreferences")]
  pub fn stream_preferences(&self, callback: StreamCallback) -> Result<StreamCloser, JsError> {
    let on_close_cb = callback.clone();
    let stream_closer = RustXmtpClient::stream_preferences_with_callback(
      self.inner_client.clone(),
      move |message| match message {
        Ok(m) => {
          callback.on_user_preference_update(m.into_iter().map(UserPreference::from).collect())
        }
        Err(e) => callback.on_error(JsError::from(e)),
      },
      move || on_close_cb.on_close(),
    );
    Ok(StreamCloser::new(stream_closer))
  }

  #[wasm_bindgen(js_name = "streamMessageDeletions")]
  pub fn stream_message_deletions(
    &self,
    callback: StreamCallback,
  ) -> Result<StreamCloser, JsError> {
    let stream_closer = RustXmtpClient::stream_message_deletions_with_callback(
      self.inner_client.clone(),
      move |message| match message {
        Ok(message_id) => callback.on_message_deleted(hex::encode(message_id)),
        Err(e) => callback.on_error(JsError::from(e)),
      },
    );
    Ok(StreamCloser::new(stream_closer))
  }
}
