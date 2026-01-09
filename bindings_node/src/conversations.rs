use crate::ErrorWrapper;
use crate::consent_state::{Consent, ConsentState};
use crate::enriched_message::DecodedMessage;
use crate::identity::Identifier;
use crate::message::Message;
use crate::permissions::{GroupPermissionsOptions, PermissionPolicySet};
use crate::{client::RustXmtpClient, conversation::Conversation, streams::StreamCloser};
use napi::bindgen_prelude::{BigInt, Error, Result, Uint8Array};
use napi::threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode};
use napi_derive::napi;
use std::collections::HashMap;
use std::ops::Deref;
use std::sync::Arc;
use xmtp_db::consent_record::ConsentState as XmtpConsentState;
use xmtp_db::group::GroupMembershipState as XmtpGroupMembershipState;
use xmtp_db::group::GroupQueryArgs;
use xmtp_db::group::{ConversationType as XmtpConversationType, GroupQueryOrderBy};
use xmtp_db::user_preferences::HmacKey as XmtpHmacKey;
use xmtp_mls::groups::ConversationDebugInfo as XmtpConversationDebugInfo;
use xmtp_mls::groups::PreconfiguredPolicies;
use xmtp_mls::groups::device_sync::preference_sync::PreferenceUpdate as XmtpUserPreferenceUpdate;
use xmtp_mls::mls_common::group::{DMMetadataOptions, GroupMetadataOptions};
use xmtp_mls::mls_common::group_mutable_metadata::MessageDisappearingSettings as XmtpMessageDisappearingSettings;
use xmtp_proto::types::Cursor as XmtpCursor;

#[napi]
#[derive(Debug, Clone, Copy)]
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
  fn from(ct: ConversationType) -> Self {
    match ct {
      ConversationType::Dm => XmtpConversationType::Dm,
      ConversationType::Group => XmtpConversationType::Group,
      ConversationType::Sync => XmtpConversationType::Sync,
      ConversationType::Oneshot => XmtpConversationType::Oneshot,
    }
  }
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
#[derive(Debug)]
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

#[napi(object)]
#[derive(Default)]
pub struct ListConversationsOptions {
  pub consent_states: Option<Vec<ConsentState>>,
  pub conversation_type: Option<ConversationType>,
  pub created_after_ns: Option<BigInt>,
  pub created_before_ns: Option<BigInt>,
  pub include_duplicate_dms: Option<bool>,
  pub limit: Option<i64>,
  pub order_by: Option<ListConversationsOrderBy>,
}

impl From<ListConversationsOptions> for GroupQueryArgs {
  fn from(opts: ListConversationsOptions) -> GroupQueryArgs {
    GroupQueryArgs {
      consent_states: opts
        .consent_states
        .map(|vec| vec.into_iter().map(Into::into).collect()),
      created_before_ns: opts.created_before_ns.map(|v| v.get_i64().0),
      created_after_ns: opts.created_after_ns.map(|v| v.get_i64().0),
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

#[napi(object)]
#[derive(Clone)]
pub struct MessageDisappearingSettings {
  pub from_ns: BigInt,
  pub in_ns: BigInt,
}

impl From<MessageDisappearingSettings> for XmtpMessageDisappearingSettings {
  fn from(value: MessageDisappearingSettings) -> Self {
    Self {
      from_ns: value.from_ns.get_i64().0,
      in_ns: value.in_ns.get_i64().0,
    }
  }
}

impl From<XmtpMessageDisappearingSettings> for MessageDisappearingSettings {
  fn from(value: XmtpMessageDisappearingSettings) -> Self {
    Self {
      from_ns: BigInt::from(value.from_ns),
      in_ns: BigInt::from(value.in_ns),
    }
  }
}

#[napi(object)]
pub struct HmacKey {
  pub key: Uint8Array,
  pub epoch: BigInt,
}

impl From<XmtpHmacKey> for HmacKey {
  fn from(value: XmtpHmacKey) -> Self {
    Self {
      epoch: BigInt::from(value.epoch),
      key: Uint8Array::from(value.key),
    }
  }
}

#[napi(object)]
pub struct ConversationDebugInfo {
  pub epoch: BigInt,
  pub maybe_forked: bool,
  pub fork_details: String,
  pub is_commit_log_forked: Option<bool>,
  pub local_commit_log: String,
  pub remote_commit_log: String,
  pub cursor: Vec<Cursor>,
}

#[napi(object)]
pub struct Cursor {
  pub originator_id: u32,
  // napi doesn't support u64
  pub sequence_id: i64,
}

impl From<XmtpCursor> for Cursor {
  fn from(value: XmtpCursor) -> Self {
    Self {
      originator_id: value.originator_id,
      sequence_id: value.sequence_id as i64,
    }
  }
}

impl From<XmtpConversationDebugInfo> for ConversationDebugInfo {
  fn from(value: XmtpConversationDebugInfo) -> Self {
    Self {
      epoch: BigInt::from(value.epoch),
      maybe_forked: value.maybe_forked,
      fork_details: value.fork_details,
      is_commit_log_forked: value.is_commit_log_forked,
      local_commit_log: value.local_commit_log,
      remote_commit_log: value.remote_commit_log,
      cursor: value.cursor.into_iter().map(Into::into).collect(),
    }
  }
}

#[napi(discriminant = "type")]
pub enum UserPreferenceUpdate {
  ConsentUpdate { consent: Consent },
  HmacKeyUpdate { key: Uint8Array },
}

impl From<XmtpUserPreferenceUpdate> for UserPreferenceUpdate {
  fn from(value: XmtpUserPreferenceUpdate) -> Self {
    match value {
      XmtpUserPreferenceUpdate::Hmac { key, .. } => Self::HmacKeyUpdate { key: key.into() },
      XmtpUserPreferenceUpdate::Consent(consent) => Self::ConsentUpdate {
        consent: consent.into(),
      },
    }
  }
}

#[napi]
pub struct ConversationListItem {
  conversation: Conversation,
  last_message: Option<Message>,
  is_commit_log_forked: Option<bool>,
}

#[napi]
impl ConversationListItem {
  #[napi(getter)]
  pub fn conversation(&self) -> Conversation {
    self.conversation.clone()
  }

  #[napi(getter)]
  pub fn last_message(&self) -> Option<Message> {
    self.last_message.clone()
  }

  #[napi(getter)]
  pub fn is_commit_log_forked(&self) -> Option<bool> {
    self.is_commit_log_forked
  }
}

#[napi(object)]
#[derive(Clone)]
pub struct CreateGroupOptions {
  pub permissions: Option<GroupPermissionsOptions>,
  pub group_name: Option<String>,
  pub group_image_url_square: Option<String>,
  pub group_description: Option<String>,
  pub custom_permission_policy_set: Option<PermissionPolicySet>,
  pub message_disappearing_settings: Option<MessageDisappearingSettings>,
  pub app_data: Option<String>,
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

#[napi(object)]
#[derive(Clone, Default)]
pub struct CreateDMOptions {
  pub message_disappearing_settings: Option<MessageDisappearingSettings>,
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

#[napi]
pub struct Conversations {
  inner_client: Arc<RustXmtpClient>,
}

#[napi]
impl Conversations {
  pub fn new(inner_client: Arc<RustXmtpClient>) -> Self {
    Self { inner_client }
  }

  #[napi]
  pub fn create_group_optimistic(
    &self,
    options: Option<CreateGroupOptions>,
  ) -> Result<Conversation> {
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
        return Err(Error::from_reason("CustomPolicy must include policy set"));
      }
    } else if options.custom_permission_policy_set.is_some() {
      return Err(Error::from_reason(
        "Only CustomPolicy may specify a policy set",
      ));
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
          Some(policy_set.try_into().map_err(ErrorWrapper::from)?)
        } else {
          None
        }
      }
      _ => None,
    };

    let group = self
      .inner_client
      .create_group(group_permissions, Some(metadata_options))
      .map_err(ErrorWrapper::from)?;

    Ok(group.into())
  }

  #[napi]
  pub async fn create_group(
    &self,
    account_identities: Vec<Identifier>,
    options: Option<CreateGroupOptions>,
  ) -> Result<Conversation> {
    let convo = self.create_group_optimistic(options)?;

    if !account_identities.is_empty() {
      convo.add_members(account_identities).await?;
    } else {
      convo.sync().await.map_err(ErrorWrapper::from)?;
    };

    Ok(convo)
  }

  #[napi]
  pub async fn create_group_by_inbox_id(
    &self,
    inbox_ids: Vec<String>,
    options: Option<CreateGroupOptions>,
  ) -> Result<Conversation> {
    let convo = self.create_group_optimistic(options)?;

    if !inbox_ids.is_empty() {
      convo.add_members_by_inbox_id(inbox_ids).await?;
    } else {
      convo.sync().await.map_err(ErrorWrapper::from)?;
    }

    Ok(convo)
  }

  #[napi(js_name = "createDm")]
  pub async fn find_or_create_dm(
    &self,
    account_identity: Identifier,
    options: Option<CreateDMOptions>,
  ) -> Result<Conversation> {
    let convo = self
      .inner_client
      .find_or_create_dm(
        account_identity.try_into()?,
        options.map(|opt| opt.into_dm_metadata_options()),
      )
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(convo.into())
  }

  #[napi(js_name = "createDmByInboxId")]
  pub async fn find_or_create_dm_by_inbox_id(
    &self,
    inbox_id: String,
    options: Option<CreateDMOptions>,
  ) -> Result<Conversation> {
    let convo = self
      .inner_client
      .find_or_create_dm_by_inbox_id(inbox_id, options.map(|opt| opt.into_dm_metadata_options()))
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(convo.into())
  }

  #[napi]
  pub fn find_group_by_id(&self, group_id: String) -> Result<Conversation> {
    let group_id = hex::decode(group_id).map_err(ErrorWrapper::from)?;

    let group = self
      .inner_client
      .stitched_group(&group_id)
      .map_err(ErrorWrapper::from)?;

    Ok(group.into())
  }

  #[napi]
  pub fn find_dm_by_target_inbox_id(&self, target_inbox_id: String) -> Result<Conversation> {
    let convo = self
      .inner_client
      .dm_group_from_target_inbox(target_inbox_id)
      .map_err(ErrorWrapper::from)?;

    Ok(convo.into())
  }

  #[napi]
  pub fn find_message_by_id(&self, message_id: String) -> Result<Message> {
    let message_id = hex::decode(message_id).map_err(ErrorWrapper::from)?;

    let message = self
      .inner_client
      .message(message_id)
      .map_err(ErrorWrapper::from)?;

    Ok(Message::from(message))
  }

  #[napi]
  pub fn find_enriched_message_by_id(&self, message_id: String) -> Result<DecodedMessage> {
    let message_id = hex::decode(message_id).map_err(ErrorWrapper::from)?;

    let message = self
      .inner_client
      .message_v2(message_id)
      .map_err(ErrorWrapper::from)?;

    message.try_into()
  }

  #[napi]
  pub fn delete_message_by_id(&self, message_id: String) -> Result<u32> {
    let message_id = hex::decode(message_id).map_err(ErrorWrapper::from)?;

    let deleted_count = self
      .inner_client
      .delete_message(message_id)
      .map_err(ErrorWrapper::from)?;

    Ok(deleted_count as u32)
  }

  #[napi]
  pub async fn process_streamed_welcome_message(
    &self,
    envelope_bytes: Uint8Array,
  ) -> Result<Vec<Conversation>> {
    let envelope_bytes = envelope_bytes.deref().to_vec();
    let group = self
      .inner_client
      .process_streamed_welcome_message(envelope_bytes)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(group.into_iter().map(Into::into).collect())
  }

  #[napi]
  pub async fn sync(&self) -> Result<()> {
    self
      .inner_client
      .sync_welcomes()
      .await
      .map_err(ErrorWrapper::from)?;
    Ok(())
  }

  #[napi]
  pub async fn sync_all_conversations(
    &self,
    consent_states: Option<Vec<ConsentState>>,
  ) -> Result<crate::client::GroupSyncSummary> {
    let consents: Option<Vec<XmtpConsentState>> =
      consent_states.map(|states| states.into_iter().map(|state| state.into()).collect());

    let summary = self
      .inner_client
      .sync_all_welcomes_and_groups(consents)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(summary.into())
  }

  #[napi]
  pub fn list(&self, opts: Option<ListConversationsOptions>) -> Result<Vec<ConversationListItem>> {
    let convo_list: Vec<ConversationListItem> = self
      .inner_client
      .list_conversations(opts.unwrap_or_default().into())
      .map_err(ErrorWrapper::from)?
      .into_iter()
      .map(|conversation_item| ConversationListItem {
        conversation: conversation_item.group.into(),
        last_message: conversation_item
          .last_message
          .map(|stored_message| stored_message.into()),
        is_commit_log_forked: conversation_item.is_commit_log_forked,
      })
      .collect();

    Ok(convo_list)
  }

  #[napi]
  pub fn get_hmac_keys(&self) -> Result<HashMap<String, Vec<HmacKey>>> {
    let inner = self.inner_client.as_ref();
    let conversations = inner
      .find_groups(GroupQueryArgs {
        include_duplicate_dms: true,
        ..Default::default()
      })
      .map_err(ErrorWrapper::from)?;

    let mut hmac_map = HashMap::new();
    for conversation in conversations {
      let id = hex::encode(&conversation.group_id);
      let keys = conversation
        .hmac_keys(-1..=1)
        .map_err(ErrorWrapper::from)?
        .into_iter()
        .map(Into::into)
        .collect::<Vec<_>>();
      hmac_map.insert(id, keys);
    }

    Ok(hmac_map)
  }

  #[napi]
  pub async fn stream(
    &self,
    callback: ThreadsafeFunction<Conversation, ()>,
    on_close: ThreadsafeFunction<(), ()>,
    conversation_type: Option<ConversationType>,
  ) -> Result<StreamCloser> {
    let stream_closer = RustXmtpClient::stream_conversations_with_callback(
      self.inner_client.clone(),
      conversation_type.map(|ct| ct.into()),
      move |convo| {
        let status = callback.call(
          convo
            .map(Conversation::from)
            .map_err(ErrorWrapper::from)
            .map_err(Error::from),
          ThreadsafeFunctionCallMode::Blocking,
        );
        tracing::info!("Stream status: {:?}", status);
      },
      move || {
        on_close.call(Ok(()), ThreadsafeFunctionCallMode::Blocking);
      },
      false,
    );

    Ok(StreamCloser::new(stream_closer))
  }

  #[napi]
  pub async fn stream_all_messages(
    &self,
    callback: ThreadsafeFunction<Message, ()>,
    on_close: ThreadsafeFunction<(), ()>,
    conversation_type: Option<ConversationType>,
    consent_states: Option<Vec<ConsentState>>,
  ) -> Result<StreamCloser> {
    tracing::trace!(
      inbox_id = self.inner_client.inbox_id(),
      conversation_type = ?conversation_type,
    );

    let inbox_id = self.inner_client.inbox_id().to_string();
    let consents: Option<Vec<XmtpConsentState>> = consent_states.map(|states| {
      states
        .into_iter()
        .map(|state: ConsentState| state.into())
        .collect()
    });

    let stream_closer = RustXmtpClient::stream_all_messages_with_callback(
      self.inner_client.context.clone(),
      conversation_type.map(Into::into),
      consents,
      move |message| {
        tracing::trace!(
            inbox_id,
            conversation_type = ?conversation_type,
            "[received] message result"
        );

        // Skip any messages that are errors
        if let Err(err) = &message {
          tracing::warn!(
            inbox_id,
            error = ?err,
            "[received] message error, swallowing to continue stream"
          );
          return; // Skip this message entirely
        }

        // For successful messages, try to transform and pass to JS
        // otherwise log error and continue stream
        match message
          .map(Into::into)
          .map_err(ErrorWrapper::from)
          .map_err(Error::from)
        {
          Ok(transformed_msg) => {
            tracing::trace!(
              inbox_id,
              "[received] calling tsfn callback with successful message"
            );
            let status = callback.call(Ok(transformed_msg), ThreadsafeFunctionCallMode::Blocking);
            tracing::info!("Stream status: {:?}", status);
          }
          Err(err) => {
            // Just in case the transformation itself fails
            tracing::error!(
              inbox_id,
              error = ?err,
              "[received] error during message transformation, swallowing to continue stream"
            );
          }
        }
      },
      move || {
        on_close.call(Ok(()), ThreadsafeFunctionCallMode::Blocking);
      },
    );

    Ok(StreamCloser::new(stream_closer))
  }

  #[napi]
  pub async fn stream_consent(
    &self,
    callback: ThreadsafeFunction<Vec<Consent>, ()>,
    on_close: ThreadsafeFunction<(), ()>,
  ) -> Result<StreamCloser> {
    tracing::trace!(inbox_id = self.inner_client.inbox_id(),);
    let inbox_id = self.inner_client.inbox_id().to_string();
    let stream_closer = RustXmtpClient::stream_consent_with_callback(
      self.inner_client.clone(),
      move |message| {
        tracing::trace!(inbox_id, "[received] calling tsfn callback");
        match message {
          Ok(message) => {
            let msg: Vec<Consent> = message.into_iter().map(Into::into).collect();
            let status = callback.call(Ok(msg), ThreadsafeFunctionCallMode::Blocking);
            tracing::info!("Stream status: {:?}", status);
          }
          Err(e) => {
            let status = callback.call(
              Err(Error::from(ErrorWrapper::from(e))),
              ThreadsafeFunctionCallMode::Blocking,
            );
            tracing::info!("Stream status: {:?}", status);
          }
        }
      },
      move || {
        on_close.call(Ok(()), ThreadsafeFunctionCallMode::Blocking);
      },
    );

    Ok(StreamCloser::new(stream_closer))
  }

  #[napi]
  pub async fn stream_preferences(
    &self,
    callback: ThreadsafeFunction<Vec<UserPreferenceUpdate>, ()>,
    on_close: ThreadsafeFunction<(), ()>,
  ) -> Result<StreamCloser> {
    tracing::trace!(inbox_id = self.inner_client.inbox_id());
    let inbox_id = self.inner_client.inbox_id().to_string();
    let stream_closer = RustXmtpClient::stream_preferences_with_callback(
      self.inner_client.clone(),
      move |message| {
        tracing::trace!(inbox_id, "[received] calling tsfn callback");
        match message {
          Ok(message) => {
            let msg: Vec<UserPreferenceUpdate> = message
              .into_iter()
              .map(UserPreferenceUpdate::from)
              .collect();
            let status = callback.call(Ok(msg), ThreadsafeFunctionCallMode::Blocking);
            tracing::info!("Stream status: {:?}", status);
          }
          Err(e) => {
            let status = callback.call(
              Err(Error::from(ErrorWrapper::from(e))),
              ThreadsafeFunctionCallMode::Blocking,
            );
            tracing::info!("Stream status: {:?}", status);
          }
        }
      },
      move || {
        let status = on_close.call(Ok(()), ThreadsafeFunctionCallMode::Blocking);
        tracing::info!("stream on close status {:?}", status);
      },
    );

    Ok(StreamCloser::new(stream_closer))
  }

  #[napi]
  pub async fn stream_message_deletions(
    &self,
    callback: ThreadsafeFunction<DecodedMessage, ()>,
  ) -> Result<StreamCloser> {
    tracing::trace!(inbox_id = self.inner_client.inbox_id());
    let stream_closer = RustXmtpClient::stream_message_deletions_with_callback(
      self.inner_client.clone(),
      move |message| match message {
        Ok(decoded_message) => match DecodedMessage::try_from(decoded_message) {
          Ok(msg) => {
            let _ = callback.call(Ok(msg), ThreadsafeFunctionCallMode::Blocking);
          }
          Err(e) => {
            let _ = callback.call(Err(e), ThreadsafeFunctionCallMode::Blocking);
          }
        },
        Err(e) => {
          let _ = callback.call(
            Err(Error::from(ErrorWrapper::from(e))),
            ThreadsafeFunctionCallMode::Blocking,
          );
        }
      },
    );

    Ok(StreamCloser::new(stream_closer))
  }
}
