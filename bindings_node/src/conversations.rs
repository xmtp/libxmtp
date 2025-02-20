use std::collections::HashMap;
use std::ops::Deref;
use std::sync::Arc;
use std::vec;

use napi::bindgen_prelude::{BigInt, Error, Result, Uint8Array};
use napi::threadsafe_function::{ErrorStrategy, ThreadsafeFunction, ThreadsafeFunctionCallMode};
use napi::JsFunction;
use napi_derive::napi;
use xmtp_mls::groups::{
  DMMetadataOptions, GroupMetadataOptions, HmacKey as XmtpHmacKey, PreconfiguredPolicies,
};
use xmtp_mls::storage::consent_record::ConsentState as XmtpConsentState;
use xmtp_mls::storage::group::ConversationType as XmtpConversationType;
use xmtp_mls::storage::group::GroupMembershipState as XmtpGroupMembershipState;
use xmtp_mls::storage::group::GroupQueryArgs;

use crate::consent_state::ConsentState;
use crate::message::Message;
use crate::permissions::{GroupPermissionsOptions, PermissionPolicySet};
use crate::ErrorWrapper;
use crate::{client::RustXmtpClient, conversation::Conversation, streams::StreamCloser};

#[napi]
#[derive(Debug)]
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
}

impl From<XmtpGroupMembershipState> for GroupMembershipState {
  fn from(gms: XmtpGroupMembershipState) -> Self {
    match gms {
      XmtpGroupMembershipState::Allowed => GroupMembershipState::Allowed,
      XmtpGroupMembershipState::Rejected => GroupMembershipState::Rejected,
      XmtpGroupMembershipState::Pending => GroupMembershipState::Pending,
    }
  }
}

impl From<GroupMembershipState> for XmtpGroupMembershipState {
  fn from(gms: GroupMembershipState) -> Self {
    match gms {
      GroupMembershipState::Allowed => XmtpGroupMembershipState::Allowed,
      GroupMembershipState::Rejected => XmtpGroupMembershipState::Rejected,
      GroupMembershipState::Pending => XmtpGroupMembershipState::Pending,
    }
  }
}

#[napi(object)]
#[derive(Debug, Default)]
pub struct ListConversationsOptions {
  pub allowed_states: Option<Vec<GroupMembershipState>>,
  pub created_after_ns: Option<i64>,
  pub created_before_ns: Option<i64>,
  pub limit: Option<i64>,
  pub conversation_type: Option<ConversationType>,
}

impl From<ListConversationsOptions> for GroupQueryArgs {
  fn from(opts: ListConversationsOptions) -> GroupQueryArgs {
    GroupQueryArgs::default()
      .maybe_allowed_states(
        opts
          .allowed_states
          .map(|states| states.into_iter().map(From::from).collect()),
      )
      .maybe_conversation_type(opts.conversation_type.map(|ct| ct.into()))
      .maybe_created_after_ns(opts.created_after_ns)
      .maybe_created_before_ns(opts.created_before_ns)
      .maybe_limit(opts.limit)
  }
}

#[napi(object)]
pub struct HmacKey {
  pub key: Vec<u8>,
  pub epoch: BigInt,
}

impl From<XmtpHmacKey> for HmacKey {
  fn from(value: XmtpHmacKey) -> Self {
    Self {
      epoch: BigInt::from(value.epoch),
      key: value.key.to_vec(),
    }
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
  pub async fn create_group(
    &self,
    account_addresses: Vec<String>,
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

    let convo = if account_addresses.is_empty() {
      let group = self
        .inner_client
        .create_group(group_permissions, metadata_options)
        .map_err(|e| Error::from_reason(format!("ClientError: {}", e)))?;
      group
        .sync()
        .await
        .map_err(|e| Error::from_reason(format!("ClientError: {}", e)))?;
      group
    } else {
      self
        .inner_client
        .create_group_with_members(&account_addresses, group_permissions, metadata_options)
        .await
        .map_err(|e| Error::from_reason(format!("ClientError: {}", e)))?
    };

    Ok(convo.into())
  }

  #[napi]
  pub async fn create_group_by_inbox_id(
    &self,
    inbox_ids: Vec<String>,
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

    let convo = if inbox_ids.is_empty() {
      let group = self
        .inner_client
        .create_group(group_permissions, metadata_options)
        .map_err(|e| Error::from_reason(format!("ClientError: {}", e)))?;
      group
        .sync()
        .await
        .map_err(|e| Error::from_reason(format!("ClientError: {}", e)))?;
      group
    } else {
      self
        .inner_client
        .create_group_with_inbox_ids(&inbox_ids, group_permissions, metadata_options)
        .await
        .map_err(|e| Error::from_reason(format!("ClientError: {}", e)))?
    };

    Ok(convo.into())
  }

  #[napi(js_name = "createDm")]
  pub async fn find_or_create_dm(
    &self,
    account_address: String,
    options: Option<CreateDMOptions>,
  ) -> Result<Conversation> {
    let convo = self
      .inner_client
      .find_or_create_dm(
        account_address,
        options.unwrap_or_default().into_dm_metadata_options(),
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
      .find_or_create_dm_by_inbox_id(
        inbox_id,
        options.unwrap_or_default().into_dm_metadata_options(),
      )
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(convo.into())
  }

  #[napi]
  pub fn find_group_by_id(&self, group_id: String) -> Result<Conversation> {
    let group_id = hex::decode(group_id).map_err(ErrorWrapper::from)?;

    let group = self
      .inner_client
      .group(group_id)
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
    let provider = self
      .inner_client
      .mls_provider()
      .map_err(ErrorWrapper::from)?;
    self
      .inner_client
      .sync_welcomes(&provider)
      .await
      .map_err(ErrorWrapper::from)?;
    Ok(())
  }

  #[napi]
  pub async fn sync_all_conversations(
    &self,
    consent_states: Option<Vec<ConsentState>>,
  ) -> Result<usize> {
    let provider = self
      .inner_client
      .mls_provider()
      .map_err(ErrorWrapper::from)?;
    let consents: Option<Vec<XmtpConsentState>> =
      consent_states.map(|states| states.into_iter().map(|state| state.into()).collect());

    let num_groups_synced = self
      .inner_client
      .sync_all_welcomes_and_groups(&provider, consents)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(num_groups_synced)
  }

  #[napi]
  pub fn list(&self, opts: Option<ListConversationsOptions>) -> Result<Vec<Conversation>> {
    let convo_list: Vec<Conversation> = self
      .inner_client
      .find_groups(opts.unwrap_or_default().into())
      .map_err(ErrorWrapper::from)?
      .into_iter()
      .map(Conversation::from)
      .collect();

    Ok(convo_list)
  }

  #[napi]
  pub fn list_groups(&self, opts: Option<ListConversationsOptions>) -> Result<Vec<Conversation>> {
    self.list(Some(ListConversationsOptions {
      conversation_type: Some(ConversationType::Group),
      ..opts.unwrap_or_default()
    }))
  }

  #[napi]
  pub fn list_dms(&self, opts: Option<ListConversationsOptions>) -> Result<Vec<Conversation>> {
    self.list(Some(ListConversationsOptions {
      conversation_type: Some(ConversationType::Dm),
      ..opts.unwrap_or_default()
    }))
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

  #[napi(ts_args_type = "callback: (err: null | Error, result: Conversation | undefined) => void")]
  pub fn stream(
    &self,
    callback: JsFunction,
    conversation_type: Option<ConversationType>,
  ) -> Result<StreamCloser> {
    let tsfn: ThreadsafeFunction<Conversation, ErrorStrategy::CalleeHandled> =
      callback.create_threadsafe_function(0, |ctx| Ok(vec![ctx.value]))?;
    let stream_closer = RustXmtpClient::stream_conversations_with_callback(
      self.inner_client.clone(),
      conversation_type.map(|ct| ct.into()),
      move |convo| {
        tsfn.call(
          convo
            .map(Conversation::from)
            .map_err(ErrorWrapper::from)
            .map_err(Error::from),
          ThreadsafeFunctionCallMode::Blocking,
        );
      },
    );

    Ok(StreamCloser::new(stream_closer))
  }

  #[napi(ts_args_type = "callback: (err: null | Error, result: Conversation | undefined) => void")]
  pub fn stream_groups(&self, callback: JsFunction) -> Result<StreamCloser> {
    self.stream(callback, Some(ConversationType::Group))
  }

  #[napi(ts_args_type = "callback: (err: null | Error, result: Conversation | undefined) => void")]
  pub fn stream_dms(&self, callback: JsFunction) -> Result<StreamCloser> {
    self.stream(callback, Some(ConversationType::Dm))
  }

  #[napi(ts_args_type = "callback: (err: null | Error, result: Message | undefined) => void")]
  pub fn stream_all_messages(
    &self,
    callback: JsFunction,
    conversation_type: Option<ConversationType>,
  ) -> Result<StreamCloser> {
    tracing::trace!(
      inbox_id = self.inner_client.inbox_id(),
      conversation_type = ?conversation_type,
    );
    let tsfn: ThreadsafeFunction<Message, ErrorStrategy::CalleeHandled> =
      callback.create_threadsafe_function(0, |ctx| Ok(vec![ctx.value]))?;
    let inbox_id = self.inner_client.inbox_id().to_string();
    let stream_closer = RustXmtpClient::stream_all_messages_with_callback(
      self.inner_client.clone(),
      conversation_type.map(Into::into),
      move |message| {
        tracing::trace!(
            inbox_id,
            conversation_type = ?conversation_type,
            "[received] calling tsfn callback"
        );
        tsfn.call(
          message
            .map(Into::into)
            .map_err(ErrorWrapper::from)
            .map_err(Error::from),
          ThreadsafeFunctionCallMode::Blocking,
        );
      },
    );

    Ok(StreamCloser::new(stream_closer))
  }

  #[napi(ts_args_type = "callback: (err: null | Error, result: Message | undefined) => void")]
  pub fn stream_all_group_messages(&self, callback: JsFunction) -> Result<StreamCloser> {
    self.stream_all_messages(callback, Some(ConversationType::Group))
  }

  #[napi(ts_args_type = "callback: (err: null | Error, result: Message | undefined) => void")]
  pub fn stream_all_dm_messages(&self, callback: JsFunction) -> Result<StreamCloser> {
    self.stream_all_messages(callback, Some(ConversationType::Dm))
  }
}
