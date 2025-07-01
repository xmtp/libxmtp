use crate::consent_state::{Consent, ConsentState};
use crate::identity::Identifier;
use crate::message::Message;
use crate::permissions::{GroupPermissionsOptions, PermissionPolicySet};
use crate::ErrorWrapper;
use crate::{client::RustXmtpClient, conversation::Conversation};
use futures::{SinkExt, StreamExt, TryStreamExt};
use napi::bindgen_prelude::{
  within_runtime_if_available, BigInt, Error, ReadableStream, Result, Uint8Array,
};
use napi::Env;
use napi_derive::napi;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ops::Deref;
use std::sync::Arc;
use std::vec;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::PollSender;
use xmtp_db::consent_record::ConsentState as XmtpConsentState;
use xmtp_db::group::ConversationType as XmtpConversationType;
use xmtp_db::group::GroupMembershipState as XmtpGroupMembershipState;
use xmtp_db::group::GroupQueryArgs;
use xmtp_db::user_preferences::HmacKey as XmtpHmacKey;
use xmtp_mls::common::group::{DMMetadataOptions, GroupMetadataOptions};
use xmtp_mls::common::group_mutable_metadata::MessageDisappearingSettings as XmtpMessageDisappearingSettings;
use xmtp_mls::groups::device_sync::preference_sync::PreferenceUpdate as XmtpUserPreferenceUpdate;
use xmtp_mls::groups::ConversationDebugInfo as XmtpConversationDebugInfo;
use xmtp_mls::groups::PreconfiguredPolicies;

#[napi]
#[derive(Clone, Copy, Debug)]
pub enum ConversationType {
  Dm = 0,
  Group = 1,
  Sync = 2,
}

impl From<XmtpConversationType> for ConversationType {
  fn from(ct: XmtpConversationType) -> Self {
    match ct {
      XmtpConversationType::Dm => ConversationType::Dm,
      XmtpConversationType::Group => ConversationType::Group,
      XmtpConversationType::Sync => ConversationType::Sync,
    }
  }
}

impl From<ConversationType> for XmtpConversationType {
  fn from(ct: ConversationType) -> Self {
    match ct {
      ConversationType::Dm => XmtpConversationType::Dm,
      ConversationType::Group => XmtpConversationType::Group,
      ConversationType::Sync => XmtpConversationType::Sync,
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
}

impl From<XmtpGroupMembershipState> for GroupMembershipState {
  fn from(gms: XmtpGroupMembershipState) -> Self {
    match gms {
      XmtpGroupMembershipState::Allowed => GroupMembershipState::Allowed,
      XmtpGroupMembershipState::Rejected => GroupMembershipState::Rejected,
      XmtpGroupMembershipState::Pending => GroupMembershipState::Pending,
      XmtpGroupMembershipState::Restored => GroupMembershipState::Restored,
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
    }
  }
}

#[napi(object)]
#[derive(Default)]
pub struct ListConversationsOptions {
  pub consent_states: Option<Vec<ConsentState>>,
  pub conversation_type: Option<ConversationType>,
  pub created_after_ns: Option<i64>,
  pub created_before_ns: Option<i64>,
  pub include_duplicate_dms: Option<bool>,
  pub limit: Option<i64>,
}

impl From<ListConversationsOptions> for GroupQueryArgs {
  fn from(opts: ListConversationsOptions) -> GroupQueryArgs {
    GroupQueryArgs {
      consent_states: opts
        .consent_states
        .map(|vec| vec.into_iter().map(Into::into).collect()),
      created_before_ns: opts.created_before_ns,
      created_after_ns: opts.created_after_ns,
      include_duplicate_dms: opts.include_duplicate_dms.unwrap_or_default(),
      limit: opts.limit,
      allowed_states: None,
      conversation_type: opts.conversation_type.map(Into::into),
      include_sync_groups: false,
      activity_after_ns: None,
    }
  }
}

#[napi(object)]
#[derive(Clone)]
pub struct MessageDisappearingSettings {
  pub from_ns: i64,
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
  pub local_commit_log: String,
  pub cursor: i64,
}

impl From<XmtpConversationDebugInfo> for ConversationDebugInfo {
  fn from(value: XmtpConversationDebugInfo) -> Self {
    Self {
      epoch: BigInt::from(value.epoch),
      maybe_forked: value.maybe_forked,
      fork_details: value.fork_details,
      local_commit_log: value.local_commit_log,
      cursor: value.cursor,
    }
  }
}

#[derive(Serialize, Deserialize)]
#[napi]
pub enum UserPreferenceUpdate {
  ConsentUpdate { consent: Consent },
  HmacKeyUpdate { key: Vec<u8> },
}

impl From<XmtpUserPreferenceUpdate> for UserPreferenceUpdate {
  fn from(value: XmtpUserPreferenceUpdate) -> Self {
    match value {
      XmtpUserPreferenceUpdate::Hmac { key, .. } => UserPreferenceUpdate::HmacKeyUpdate { key },
      XmtpUserPreferenceUpdate::Consent(consent) => UserPreferenceUpdate::ConsentUpdate {
        consent: Consent::from(consent),
      },
    }
  }
}

#[napi]
pub struct ConversationListItem {
  conversation: Conversation,
  last_message: Option<Message>,
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
          Some(
            policy_set
              .try_into()
              .map_err(|e| Error::from_reason(format!("{}", e).as_str()))?,
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
      .map_err(|e| Error::from_reason(format!("ClientError: {}", e)))?;

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
      convo
        .sync()
        .await
        .map_err(|e| Error::from_reason(format!("ClientError: {}", e)))?;
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
      convo
        .sync()
        .await
        .map_err(|e| Error::from_reason(format!("ClientError: {}", e)))?;
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
  pub async fn process_streamed_welcome_message(
    &self,
    envelope_bytes: Uint8Array,
  ) -> Result<Conversation> {
    let envelope_bytes = envelope_bytes.deref().to_vec();
    let group = self
      .inner_client
      .process_streamed_welcome_message(envelope_bytes)
      .await
      .map_err(ErrorWrapper::from)?;
    Ok(group.into())
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
  ) -> Result<usize> {
    let consents: Option<Vec<XmtpConsentState>> =
      consent_states.map(|states| states.into_iter().map(|state| state.into()).collect());

    let num_groups_synced = self
      .inner_client
      .sync_all_welcomes_and_groups(consents)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(num_groups_synced)
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
  pub fn stream(
    &self,
    env: Env,
    conversation_type: Option<ConversationType>,
  ) -> Result<ReadableStream<'_, Conversation>> {
    let (tx, rx) = mpsc::channel(64);

    let client = self.inner_client.clone();
    within_runtime_if_available(move || {
      tokio::spawn(async move {
        let tx =
          PollSender::new(tx).sink_map_err(|_| Error::from_reason("stream sink channel closed"));
        client
          .stream_conversations_owned(conversation_type.map(|ct| ct.into()))
          .await
          .map_err(ErrorWrapper::from)?
          // wrap with extra ok so forward never fails
          // https://github.com/rust-lang/futures-rs/issues/2004
          .map_ok(|c| Ok(Conversation::from(c)))
          .map_err(|e| Error::from(ErrorWrapper::from(e)))
          .forward(tx)
          .await?;
        Ok::<_, napi::Error>(())
      })
    });
    let stream = ReceiverStream::new(rx);
    ReadableStream::new(&env, stream)
  }

  #[napi]
  pub fn stream_all_messages(
    &self,
    env: Env,
    conversation_type: Option<ConversationType>,
    consent_states: Option<Vec<ConsentState>>,
  ) -> Result<ReadableStream<'_, Message>> {
    let (tx, rx) = mpsc::channel(64);
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

    let client = self.inner_client.clone();
    within_runtime_if_available(move || {
      tokio::spawn(async move {
        let tx =
          PollSender::new(tx).sink_map_err(|_| Error::from_reason("stream sink channel closed"));
        client
          .stream_all_messages_owned(conversation_type.map(Into::into), consents)
          .await
          .map_err(ErrorWrapper::from)?
          .filter_map(move |message| {
            let inbox_id = inbox_id.clone();
            async move {
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
                return None; // Skip this message entirely
              }

              // For successful messages, try to transform and pass to JS
              // otherwise log error and continue stream
              match message
                .map(Message::from)
                .map_err(ErrorWrapper::from)
                .map_err(Error::from)
              {
                Ok(transformed_msg) => {
                  tracing::trace!(inbox_id, "[received] returning successful message");
                  Some(Ok(transformed_msg))
                }
                Err(err) => {
                  // Just in case the transformation itself fails
                  tracing::error!(
                    inbox_id,
                    error = ?err,
                    "[received] error during message transformation, swallowing to continue stream"
                  );
                  None
                }
              }
            }
          })
          // wrap with extra ok so forward future never fails and passes error to stream
          // https://github.com/rust-lang/futures-rs/issues/2004
          .map(|i| Ok(i))
          .forward(tx)
          .await?;
        Ok::<_, napi::Error>(())
      })
    });
    let stream = ReceiverStream::new(rx);
    ReadableStream::new(&env, stream)
  }

  #[napi]
  pub fn stream_consent(&self, env: Env) -> Result<ReadableStream<'_, Vec<Consent>>> {
    tracing::trace!(inbox_id = self.inner_client.inbox_id(),);
    let stream = self
      .inner_client
      .stream_consent()
      .map_ok(|message| message.into_iter().map(Into::into).collect())
      .map_err(|e| Error::from(ErrorWrapper::from(e)));

    ReadableStream::new(&env, Box::pin(stream))
  }

  #[napi]
  pub fn stream_preferences(
    &self,
    env: Env,
  ) -> Result<ReadableStream<'_, Vec<UserPreferenceUpdate>>> {
    let stream = self
      .inner_client
      .stream_preferences()
      .map_ok(|pref| {
        pref
          .into_iter()
          .map(UserPreferenceUpdate::from)
          .collect::<Vec<UserPreferenceUpdate>>()
      })
      .map_err(|e| Error::from(ErrorWrapper::from(e)));

    tracing::trace!(inbox_id = self.inner_client.inbox_id(),);
    ReadableStream::new(&env, Box::pin(stream))
  }
}
