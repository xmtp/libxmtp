use std::{ops::Deref, sync::Arc};

use napi::{
  bindgen_prelude::{Result, Uint8Array},
  threadsafe_function::{ErrorStrategy, ThreadsafeFunction, ThreadsafeFunctionCallMode},
  JsFunction,
};
use xmtp_db::{group::ConversationType, group_message::MsgQueryArgs};
use xmtp_mls::groups::{
  group_metadata::GroupMetadata as XmtpGroupMetadata,
  group_mutable_metadata::MetadataField as XmtpMetadataField,
  intents::PermissionUpdateType as XmtpPermissionUpdateType,
  members::PermissionLevel as XmtpPermissionLevel, MlsGroup, UpdateAdminListType,
};

use xmtp_proto::xmtp::mls::message_contents::EncodedContent as XmtpEncodedContent;

use crate::{
  client::RustXmtpClient,
  consent_state::ConsentState,
  conversations::{HmacKey, MessageDisappearingSettings},
  encoded_content::EncodedContent,
  identity::{Identifier, IdentityExt},
  message::{ListMessagesOptions, Message, MessageWithReactions},
  permissions::{GroupPermissions, MetadataField, PermissionPolicy, PermissionUpdateType},
  streams::StreamCloser,
  ErrorWrapper,
};
use prost::Message as ProstMessage;

use crate::conversations::ConversationDebugInfo;
use napi_derive::napi;

#[napi]
pub struct GroupMetadata {
  inner: XmtpGroupMetadata,
}

#[napi]
impl GroupMetadata {
  #[napi]
  pub fn creator_inbox_id(&self) -> String {
    self.inner.creator_inbox_id.clone()
  }

  #[napi]
  pub fn conversation_type(&self) -> String {
    match self.inner.conversation_type {
      ConversationType::Group => "group".to_string(),
      ConversationType::Dm => "dm".to_string(),
      ConversationType::Sync => "sync".to_string(),
    }
  }
}

#[napi]
pub enum PermissionLevel {
  Member,
  Admin,
  SuperAdmin,
}

#[napi]
pub struct GroupMember {
  pub inbox_id: String,
  pub account_identifiers: Vec<Identifier>,
  pub installation_ids: Vec<String>,
  pub permission_level: PermissionLevel,
  pub consent_state: ConsentState,
}

#[napi]
#[derive(Clone)]
pub struct Conversation {
  inner_client: Arc<RustXmtpClient>,
  group_id: Vec<u8>,
  created_at_ns: i64,
}

impl From<MlsGroup<RustXmtpClient>> for Conversation {
  fn from(mls_group: MlsGroup<RustXmtpClient>) -> Self {
    Conversation {
      group_id: mls_group.group_id,
      created_at_ns: mls_group.created_at_ns,
      inner_client: mls_group.client,
    }
  }
}

#[napi]
impl Conversation {
  pub fn new(inner_client: Arc<RustXmtpClient>, group_id: Vec<u8>, created_at_ns: i64) -> Self {
    Self {
      inner_client,
      group_id,
      created_at_ns,
    }
  }

  #[napi]
  pub fn id(&self) -> String {
    match self.inner_client.stitched_group(&self.group_id) {
      Ok(group) => hex::encode(group.group_id.clone()),
      Err(_) => hex::encode(self.group_id.clone()),
    }
  }

  #[napi]
  pub async fn send(&self, encoded_content: EncodedContent) -> Result<String> {
    let encoded_content: XmtpEncodedContent = encoded_content.into();
    let group = MlsGroup::new(
      self.inner_client.clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );

    let message_id = group
      .send_message(encoded_content.encode_to_vec().as_slice())
      .await
      .map_err(ErrorWrapper::from)?;
    Ok(hex::encode(message_id.clone()))
  }

  #[napi]
  pub fn send_optimistic(&self, encoded_content: EncodedContent) -> Result<String> {
    let encoded_content: XmtpEncodedContent = encoded_content.into();
    let group = MlsGroup::new(
      self.inner_client.clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );

    let id = group
      .send_message_optimistic(encoded_content.encode_to_vec().as_slice())
      .map_err(ErrorWrapper::from)?;

    Ok(hex::encode(id.clone()))
  }

  #[napi]
  pub async fn publish_messages(&self) -> Result<()> {
    let group = MlsGroup::new(
      self.inner_client.clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );
    group.publish_messages().await.map_err(ErrorWrapper::from)?;
    Ok(())
  }

  #[napi]
  pub async fn sync(&self) -> Result<()> {
    let group = MlsGroup::new(
      self.inner_client.clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );

    group.sync().await.map_err(ErrorWrapper::from)?;

    Ok(())
  }

  #[napi]
  pub async fn find_messages(&self, opts: Option<ListMessagesOptions>) -> Result<Vec<Message>> {
    let opts = opts.unwrap_or_default();
    let group = MlsGroup::new(
      self.inner_client.clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );
    let provider = group.mls_provider().map_err(ErrorWrapper::from)?;
    let conversation_type = group
      .conversation_type(&provider)
      .await
      .map_err(ErrorWrapper::from)?;
    let kind = match conversation_type {
      ConversationType::Group => None,
      ConversationType::Dm => None,
      ConversationType::Sync => None,
    };
    let opts = MsgQueryArgs {
      kind,
      ..opts.into()
    };
    let messages: Vec<Message> = group
      .find_messages(&opts)
      .map_err(ErrorWrapper::from)?
      .into_iter()
      .map(|msg| msg.into())
      .collect();

    Ok(messages)
  }

  #[napi]
  pub async fn find_messages_with_reactions(
    &self,
    opts: Option<ListMessagesOptions>,
  ) -> Result<Vec<MessageWithReactions>> {
    let opts = opts.unwrap_or_default();
    let group = MlsGroup::new(
      self.inner_client.clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );
    let provider = group.mls_provider().map_err(ErrorWrapper::from)?;
    let conversation_type = group
      .conversation_type(&provider)
      .await
      .map_err(ErrorWrapper::from)?;
    let kind = match conversation_type {
      ConversationType::Group => None,
      ConversationType::Dm => None,
      ConversationType::Sync => None,
    };
    let opts = MsgQueryArgs {
      kind,
      ..opts.into()
    };

    let messages: Vec<MessageWithReactions> = group
      .find_messages_with_reactions(&opts)
      .map_err(ErrorWrapper::from)?
      .into_iter()
      .map(Into::into)
      .collect();

    Ok(messages)
  }

  #[napi]
  pub async fn process_streamed_group_message(
    &self,
    envelope_bytes: Uint8Array,
  ) -> Result<Message> {
    let group = MlsGroup::new(
      self.inner_client.clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );
    let envelope_bytes: Vec<u8> = envelope_bytes.deref().to_vec();
    let message = group
      .process_streamed_group_message(envelope_bytes)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(message.into())
  }

  #[napi]
  pub async fn list_members(&self) -> Result<Vec<GroupMember>> {
    let group = MlsGroup::new(
      self.inner_client.clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );

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
  pub fn admin_list(&self) -> Result<Vec<String>> {
    let group = MlsGroup::new(
      self.inner_client.clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );

    let admin_list = group
      .admin_list(&group.mls_provider().map_err(ErrorWrapper::from)?)
      .map_err(ErrorWrapper::from)?;

    Ok(admin_list)
  }

  #[napi]
  pub fn super_admin_list(&self) -> Result<Vec<String>> {
    let group = MlsGroup::new(
      self.inner_client.clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );

    let super_admin_list = group
      .super_admin_list(&group.mls_provider().map_err(ErrorWrapper::from)?)
      .map_err(ErrorWrapper::from)?;

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
    let group = MlsGroup::new(
      self.inner_client.clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );

    group
      .add_members(&account_identities.to_internal()?)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }

  #[napi]
  pub async fn add_admin(&self, inbox_id: String) -> Result<()> {
    let group = MlsGroup::new(
      self.inner_client.clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );
    group
      .update_admin_list(UpdateAdminListType::Add, inbox_id)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }

  #[napi]
  pub async fn remove_admin(&self, inbox_id: String) -> Result<()> {
    let group = MlsGroup::new(
      self.inner_client.clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );
    group
      .update_admin_list(UpdateAdminListType::Remove, inbox_id)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }

  #[napi]
  pub async fn add_super_admin(&self, inbox_id: String) -> Result<()> {
    let group = MlsGroup::new(
      self.inner_client.clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );
    group
      .update_admin_list(UpdateAdminListType::AddSuper, inbox_id)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }

  #[napi]
  pub async fn remove_super_admin(&self, inbox_id: String) -> Result<()> {
    let group = MlsGroup::new(
      self.inner_client.clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );
    group
      .update_admin_list(UpdateAdminListType::RemoveSuper, inbox_id)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }

  #[napi]
  pub fn group_permissions(&self) -> Result<GroupPermissions> {
    let group = MlsGroup::new(
      self.inner_client.clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );

    let permissions = group.permissions().map_err(ErrorWrapper::from)?;

    Ok(GroupPermissions::new(permissions))
  }

  #[napi]
  pub async fn add_members_by_inbox_id(&self, inbox_ids: Vec<String>) -> Result<()> {
    let group = MlsGroup::new(
      self.inner_client.clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );

    group
      .add_members_by_inbox_id(&inbox_ids)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }

  #[napi]
  pub async fn remove_members(&self, account_identities: Vec<Identifier>) -> Result<()> {
    let group = MlsGroup::new(
      self.inner_client.clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );

    group
      .remove_members(&account_identities.to_internal()?)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }

  #[napi]
  pub async fn remove_members_by_inbox_id(&self, inbox_ids: Vec<String>) -> Result<()> {
    let group = MlsGroup::new(
      self.inner_client.clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );

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
  pub async fn update_group_name(&self, group_name: String) -> Result<()> {
    let group = MlsGroup::new(
      self.inner_client.clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );

    group
      .update_group_name(group_name)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }

  #[napi]
  pub fn group_name(&self) -> Result<String> {
    let group = MlsGroup::new(
      self.inner_client.clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );

    let group_name = group
      .group_name(&group.mls_provider().map_err(ErrorWrapper::from)?)
      .map_err(ErrorWrapper::from)?;

    Ok(group_name)
  }

  #[napi]
  pub async fn update_group_image_url_square(&self, group_image_url_square: String) -> Result<()> {
    let group = MlsGroup::new(
      self.inner_client.clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );

    group
      .update_group_image_url_square(group_image_url_square)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }

  #[napi]
  pub fn group_image_url_square(&self) -> Result<String> {
    let group = MlsGroup::new(
      self.inner_client.clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );

    let group_image_url_square = group
      .group_image_url_square(&group.mls_provider().map_err(ErrorWrapper::from)?)
      .map_err(ErrorWrapper::from)?;

    Ok(group_image_url_square)
  }

  #[napi]
  pub async fn update_group_description(&self, group_description: String) -> Result<()> {
    let group = MlsGroup::new(
      self.inner_client.clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );

    group
      .update_group_description(group_description)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }

  #[napi]
  pub fn group_description(&self) -> Result<String> {
    let group = MlsGroup::new(
      self.inner_client.clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );

    let group_description = group
      .group_description(&group.mls_provider().map_err(ErrorWrapper::from)?)
      .map_err(ErrorWrapper::from)?;

    Ok(group_description)
  }

  #[napi(ts_args_type = "callback: (err: null | Error, result: Message | undefined) => void")]
  pub fn stream(&self, callback: JsFunction) -> Result<StreamCloser> {
    let tsfn: ThreadsafeFunction<Message, ErrorStrategy::CalleeHandled> =
      callback.create_threadsafe_function(0, |ctx| Ok(vec![ctx.value]))?;
    let stream_closer = MlsGroup::stream_with_callback(
      self.inner_client.clone(),
      self.group_id.clone(),
      move |message| {
        tsfn.call(
          message
            .map(Message::from)
            .map_err(ErrorWrapper::from)
            .map_err(napi::Error::from),
          ThreadsafeFunctionCallMode::Blocking,
        );
      },
    );

    Ok(StreamCloser::new(stream_closer))
  }

  #[napi]
  pub fn created_at_ns(&self) -> i64 {
    self.created_at_ns
  }

  #[napi]
  pub fn is_active(&self) -> Result<bool> {
    let group = MlsGroup::new(
      self.inner_client.clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );

    Ok(
      group
        .is_active(&group.mls_provider().map_err(ErrorWrapper::from)?)
        .map_err(ErrorWrapper::from)?,
    )
  }

  #[napi]
  pub fn paused_for_version(&self) -> napi::Result<Option<String>> {
    let group = MlsGroup::new(
      self.inner_client.clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );

    Ok(
      group
        .paused_for_version(&group.mls_provider().map_err(ErrorWrapper::from)?)
        .map_err(ErrorWrapper::from)?,
    )
  }

  #[napi]
  pub fn added_by_inbox_id(&self) -> Result<String> {
    let group = MlsGroup::new(
      self.inner_client.clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );

    Ok(group.added_by_inbox_id().map_err(ErrorWrapper::from)?)
  }

  #[napi]
  pub async fn group_metadata(&self) -> Result<GroupMetadata> {
    let group = MlsGroup::new(
      self.inner_client.clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );

    let metadata = group
      .metadata(&group.mls_provider().map_err(ErrorWrapper::from)?)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(GroupMetadata { inner: metadata })
  }

  #[napi]
  pub fn consent_state(&self) -> Result<ConsentState> {
    let group = MlsGroup::new(
      self.inner_client.clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );

    let state = group.consent_state().map_err(ErrorWrapper::from)?;

    Ok(state.into())
  }

  #[napi]
  pub fn update_consent_state(&self, state: ConsentState) -> Result<()> {
    let group = MlsGroup::new(
      self.inner_client.clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );

    group
      .update_consent_state(state.into())
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }

  #[napi]
  pub fn dm_peer_inbox_id(&self) -> Result<String> {
    let group = MlsGroup::new(
      self.inner_client.clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );

    Ok(group.dm_inbox_id().map_err(ErrorWrapper::from)?)
  }

  #[napi]
  pub async fn update_permission_policy(
    &self,
    permission_update_type: PermissionUpdateType,
    permission_policy_option: PermissionPolicy,
    metadata_field: Option<MetadataField>,
  ) -> Result<()> {
    let group = MlsGroup::new(
      self.inner_client.clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );

    group
      .update_permission_policy(
        XmtpPermissionUpdateType::from(&permission_update_type),
        permission_policy_option
          .try_into()
          .map_err(ErrorWrapper::from)?,
        metadata_field.map(|field| XmtpMetadataField::from(&field)),
      )
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }

  #[napi]
  pub async fn update_message_disappearing_settings(
    &self,
    settings: MessageDisappearingSettings,
  ) -> Result<()> {
    let group = MlsGroup::new(
      self.inner_client.clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );
    group
      .update_conversation_message_disappearing_settings(settings.into())
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }

  #[napi]
  pub async fn remove_message_disappearing_settings(&self) -> Result<()> {
    let group = MlsGroup::new(
      self.inner_client.clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );

    group
      .remove_conversation_message_disappearing_settings()
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }

  #[napi]
  pub fn message_disappearing_settings(&self) -> Result<Option<MessageDisappearingSettings>> {
    let settings = self
      .inner_client
      .group_disappearing_settings(self.group_id.clone())
      .map_err(ErrorWrapper::from)?;

    match settings {
      Some(s) => Ok(Some(s.into())),
      None => Ok(None),
    }
  }

  #[napi]
  pub fn is_message_disappearing_enabled(&self) -> Result<bool> {
    self.message_disappearing_settings().map(|settings| {
      settings
        .as_ref()
        .is_some_and(|s| s.from_ns > 0 && s.in_ns > 0)
    })
  }

  #[napi]
  pub fn get_hmac_keys(&self) -> Result<Vec<HmacKey>> {
    let group = MlsGroup::new(
      self.inner_client.clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );

    group
      .hmac_keys(-1..=1)
      .map(|keys| keys.into_iter().map(Into::into).collect())
      .map_err(|e| napi::Error::from_reason(e.to_string()))
  }

  #[napi]
  pub async fn debug_info(&self) -> Result<ConversationDebugInfo> {
    let group = MlsGroup::new(
      self.inner_client.clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );

    group
      .debug_info()
      .await
      .map(Into::into)
      .map_err(|e| napi::Error::from_reason(e.to_string()))
  }
}
