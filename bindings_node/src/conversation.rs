use std::{collections::HashMap, ops::Deref};

use napi::{
  bindgen_prelude::{BigInt, Result, Uint8Array},
  threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode},
};
use xmtp_content_types::{
  actions::ActionsCodec,
  attachment::AttachmentCodec,
  intent::IntentCodec,
  markdown::MarkdownCodec,
  multi_remote_attachment::MultiRemoteAttachmentCodec,
  reaction::ReactionCodec,
  read_receipt::{ReadReceipt, ReadReceiptCodec},
  remote_attachment::RemoteAttachmentCodec,
  reply::ReplyCodec,
  text::TextCodec,
  transaction_reference::TransactionReferenceCodec,
  wallet_send_calls::WalletSendCallsCodec,
};
use xmtp_db::{group::DmIdExt, group_message::MsgQueryArgs};

use xmtp_content_types::ContentCodec;

use crate::{
  content_types::{
    actions::Actions, attachment::Attachment, intent::Intent,
    multi_remote_attachment::MultiRemoteAttachment, reaction::Reaction,
    remote_attachment::RemoteAttachment, reply::Reply, transaction_reference::TransactionReference,
    wallet_send_calls::WalletSendCalls,
  },
  conversations::{ConversationType, GroupMembershipState},
};
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

use crate::{
  ErrorWrapper,
  client::RustMlsGroup,
  consent_state::ConsentState,
  conversations::{HmacKey, MessageDisappearingSettings},
  encoded_content::EncodedContent,
  enriched_message::DecodedMessage,
  identity::{Identifier, IdentityExt},
  message::{ListMessagesOptions, Message},
  permissions::{GroupPermissions, MetadataField, PermissionPolicy, PermissionUpdateType},
  streams::StreamCloser,
};
use prost::Message as ProstMessage;

use crate::conversations::ConversationDebugInfo;
use napi_derive::napi;

#[napi(object)]
pub struct SendMessageOpts {
  pub should_push: bool,
  pub optimistic: Option<bool>,
}

impl From<SendMessageOpts> for xmtp_mls::groups::send_message_opts::SendMessageOpts {
  fn from(opts: SendMessageOpts) -> Self {
    xmtp_mls::groups::send_message_opts::SendMessageOpts {
      should_push: opts.should_push,
    }
  }
}

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
  pub fn conversation_type(&self) -> ConversationType {
    self.inner.conversation_type.into()
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
#[derive(Clone)]
pub struct Conversation {
  inner_group: RustMlsGroup,
  group_id: Vec<u8>,
  dm_id: Option<String>,
  created_at_ns: BigInt,
}

impl From<RustMlsGroup> for Conversation {
  fn from(mls_group: RustMlsGroup) -> Self {
    Conversation {
      group_id: mls_group.group_id.clone(),
      dm_id: mls_group.dm_id.clone(),
      created_at_ns: BigInt::from(mls_group.created_at_ns),
      inner_group: mls_group,
    }
  }
}

#[napi]
impl Conversation {
  pub fn new(
    inner_group: RustMlsGroup,
    group_id: Vec<u8>,
    dm_id: Option<String>,
    created_at_ns: BigInt,
  ) -> Self {
    Self {
      inner_group,
      group_id,
      dm_id,
      created_at_ns,
    }
  }

  // Private helper method to create a new MlsGroup
  fn create_mls_group(&self) -> RustMlsGroup {
    MlsGroup::new(
      self.inner_group.context.clone(),
      self.group_id.clone(),
      self.dm_id.clone(),
      self.inner_group.conversation_type,
      self.created_at_ns.get_i64().0,
    )
  }

  #[napi]
  pub fn id(&self) -> String {
    hex::encode(self.group_id.clone())
  }

  #[napi]
  pub async fn send(
    &self,
    encoded_content: EncodedContent,
    opts: SendMessageOpts,
  ) -> Result<String> {
    let encoded_content: XmtpEncodedContent = encoded_content.into();
    let group = self.create_mls_group();

    let message_id = match opts.optimistic {
      Some(true) => group
        .send_message_optimistic(encoded_content.encode_to_vec().as_slice(), opts.into())
        .map_err(ErrorWrapper::from)?,
      _ => group
        .send_message(encoded_content.encode_to_vec().as_slice(), opts.into())
        .await
        .map_err(ErrorWrapper::from)?,
    };

    Ok(hex::encode(message_id))
  }

  #[napi]
  pub async fn send_text(&self, text: String, optimistic: Option<bool>) -> Result<String> {
    let encoded_content = TextCodec::encode(text).map_err(ErrorWrapper::from)?;
    let opts = SendMessageOpts {
      should_push: TextCodec::should_push(),
      optimistic,
    };
    self.send(encoded_content.into(), opts).await
  }

  #[napi]
  pub async fn send_markdown(&self, markdown: String, optimistic: Option<bool>) -> Result<String> {
    let encoded_content = MarkdownCodec::encode(markdown).map_err(ErrorWrapper::from)?;
    let opts = SendMessageOpts {
      should_push: MarkdownCodec::should_push(),
      optimistic,
    };
    self.send(encoded_content.into(), opts).await
  }

  #[napi]
  pub async fn send_reaction(
    &self,
    reaction: Reaction,
    optimistic: Option<bool>,
  ) -> Result<String> {
    let encoded_content = ReactionCodec::encode(reaction.into()).map_err(ErrorWrapper::from)?;
    let opts = SendMessageOpts {
      should_push: ReactionCodec::should_push(),
      optimistic,
    };
    self.send(encoded_content.into(), opts).await
  }

  #[napi]
  pub async fn send_reply(&self, reply: Reply, optimistic: Option<bool>) -> Result<String> {
    let encoded_content = ReplyCodec::encode(reply.into()).map_err(ErrorWrapper::from)?;
    let opts = SendMessageOpts {
      should_push: ReplyCodec::should_push(),
      optimistic,
    };
    self.send(encoded_content.into(), opts).await
  }

  #[napi]
  pub async fn send_read_receipt(&self, optimistic: Option<bool>) -> Result<String> {
    let encoded_content = ReadReceiptCodec::encode(ReadReceipt {}).map_err(ErrorWrapper::from)?;
    let opts = SendMessageOpts {
      should_push: ReadReceiptCodec::should_push(),
      optimistic,
    };
    self.send(encoded_content.into(), opts).await
  }

  #[napi]
  pub async fn send_attachment(
    &self,
    attachment: Attachment,
    optimistic: Option<bool>,
  ) -> Result<String> {
    let encoded_content = AttachmentCodec::encode(attachment.into()).map_err(ErrorWrapper::from)?;
    let opts = SendMessageOpts {
      should_push: AttachmentCodec::should_push(),
      optimistic,
    };
    self.send(encoded_content.into(), opts).await
  }

  #[napi]
  pub async fn send_remote_attachment(
    &self,
    remote_attachment: RemoteAttachment,
    optimistic: Option<bool>,
  ) -> Result<String> {
    let encoded_content =
      RemoteAttachmentCodec::encode(remote_attachment.into()).map_err(ErrorWrapper::from)?;
    let opts = SendMessageOpts {
      should_push: RemoteAttachmentCodec::should_push(),
      optimistic,
    };
    self.send(encoded_content.into(), opts).await
  }

  #[napi]
  pub async fn send_multi_remote_attachment(
    &self,
    multi_remote_attachment: MultiRemoteAttachment,
    optimistic: Option<bool>,
  ) -> Result<String> {
    let encoded_content = MultiRemoteAttachmentCodec::encode(multi_remote_attachment.into())
      .map_err(ErrorWrapper::from)?;
    let opts = SendMessageOpts {
      should_push: MultiRemoteAttachmentCodec::should_push(),
      optimistic,
    };
    self.send(encoded_content.into(), opts).await
  }

  #[napi]
  pub async fn send_transaction_reference(
    &self,
    transaction_reference: TransactionReference,
    optimistic: Option<bool>,
  ) -> Result<String> {
    let encoded_content = TransactionReferenceCodec::encode(transaction_reference.into())
      .map_err(ErrorWrapper::from)?;
    let opts = SendMessageOpts {
      should_push: TransactionReferenceCodec::should_push(),
      optimistic,
    };
    self.send(encoded_content.into(), opts).await
  }

  #[napi]
  pub async fn send_wallet_send_calls(
    &self,
    wallet_send_calls: WalletSendCalls,
    optimistic: Option<bool>,
  ) -> Result<String> {
    let wsc = wallet_send_calls.try_into()?;
    let encoded_content = WalletSendCallsCodec::encode(wsc).map_err(ErrorWrapper::from)?;
    let opts = SendMessageOpts {
      should_push: WalletSendCallsCodec::should_push(),
      optimistic,
    };
    self.send(encoded_content.into(), opts).await
  }

  #[napi]
  pub async fn send_actions(&self, actions: Actions, optimistic: Option<bool>) -> Result<String> {
    let encoded_content = ActionsCodec::encode(actions.into()).map_err(ErrorWrapper::from)?;
    let opts = SendMessageOpts {
      should_push: ActionsCodec::should_push(),
      optimistic,
    };
    self.send(encoded_content.into(), opts).await
  }

  #[napi]
  pub async fn send_intent(&self, intent: Intent, optimistic: Option<bool>) -> Result<String> {
    let encoded_content = IntentCodec::encode(intent.into()).map_err(ErrorWrapper::from)?;
    let opts = SendMessageOpts {
      should_push: IntentCodec::should_push(),
      optimistic,
    };
    self.send(encoded_content.into(), opts).await
  }

  #[napi]
  pub async fn publish_messages(&self) -> Result<()> {
    let group = self.create_mls_group();
    group.publish_messages().await.map_err(ErrorWrapper::from)?;
    Ok(())
  }

  #[napi]
  pub async fn sync(&self) -> Result<()> {
    let group = self.create_mls_group();
    group.sync().await.map_err(ErrorWrapper::from)?;

    Ok(())
  }

  #[napi]
  pub async fn find_messages(&self, opts: Option<ListMessagesOptions>) -> Result<Vec<Message>> {
    let opts = opts.unwrap_or_default();
    let group = self.create_mls_group();
    let opts = MsgQueryArgs { ..opts.into() };
    let messages: Vec<Message> = group
      .find_messages(&opts)
      .map_err(ErrorWrapper::from)?
      .into_iter()
      .map(|msg| msg.into())
      .collect();

    Ok(messages)
  }

  #[napi]
  pub async fn count_messages(&self, opts: Option<ListMessagesOptions>) -> Result<i64> {
    let opts = opts.unwrap_or_default();
    let group = self.create_mls_group();
    let msg_args: MsgQueryArgs = opts.into();
    let count = group
      .count_messages(&msg_args)
      .map_err(ErrorWrapper::from)?;

    Ok(count)
  }

  #[napi]
  pub async fn process_streamed_group_message(
    &self,
    envelope_bytes: Uint8Array,
  ) -> Result<Vec<Message>> {
    let group = self.create_mls_group();
    let envelope_bytes: Vec<u8> = envelope_bytes.deref().to_vec();
    let message = group
      .process_streamed_group_message(envelope_bytes)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(message.into_iter().map(Into::into).collect())
  }

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
  pub async fn update_group_name(&self, group_name: String) -> Result<()> {
    let group = self.create_mls_group();

    group
      .update_group_name(group_name)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }

  #[napi]
  pub fn group_name(&self) -> Result<String> {
    let group = self.create_mls_group();

    let group_name = group.group_name().map_err(ErrorWrapper::from)?;

    Ok(group_name)
  }

  #[napi]
  pub async fn update_app_data(&self, app_data: String) -> Result<()> {
    let group = self.create_mls_group();

    group
      .update_app_data(app_data)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }

  #[napi]
  pub fn app_data(&self) -> Result<String> {
    let group = self.create_mls_group();

    let app_data = group.app_data().map_err(ErrorWrapper::from)?;

    Ok(app_data)
  }

  #[napi]
  pub async fn update_group_image_url_square(&self, group_image_url_square: String) -> Result<()> {
    let group = self.create_mls_group();

    group
      .update_group_image_url_square(group_image_url_square)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }

  #[napi]
  pub fn group_image_url_square(&self) -> Result<String> {
    let group = self.create_mls_group();

    let group_image_url_square = group.group_image_url_square().map_err(ErrorWrapper::from)?;

    Ok(group_image_url_square)
  }

  #[napi]
  pub async fn update_group_description(&self, group_description: String) -> Result<()> {
    let group = self.create_mls_group();

    group
      .update_group_description(group_description)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }

  #[napi]
  pub fn group_description(&self) -> Result<String> {
    let group = self.create_mls_group();

    let group_description = group.group_description().map_err(ErrorWrapper::from)?;

    Ok(group_description)
  }

  #[napi]
  pub async fn stream(
    &self,
    callback: ThreadsafeFunction<Message, ()>,
    on_close: ThreadsafeFunction<(), ()>,
  ) -> Result<StreamCloser> {
    let stream_closer = MlsGroup::stream_with_callback(
      self.inner_group.context.clone(),
      self.group_id.clone(),
      move |message| {
        let status = callback.call(
          message
            .map(Message::from)
            .map_err(ErrorWrapper::from)
            .map_err(napi::Error::from),
          ThreadsafeFunctionCallMode::Blocking,
        );
        tracing::info!("Stream status: {:?}", status);
      },
      move || {
        on_close.call(Ok(()), ThreadsafeFunctionCallMode::Blocking);
      },
    );

    Ok(StreamCloser::new(stream_closer))
  }

  #[napi]
  pub fn created_at_ns(&self) -> BigInt {
    self.created_at_ns.clone()
  }

  #[napi]
  pub fn is_active(&self) -> Result<bool> {
    let group = self.create_mls_group();

    Ok(group.is_active().map_err(ErrorWrapper::from)?)
  }

  #[napi]
  pub fn paused_for_version(&self) -> napi::Result<Option<String>> {
    let group = self.create_mls_group();

    Ok(group.paused_for_version().map_err(ErrorWrapper::from)?)
  }

  #[napi]
  pub fn added_by_inbox_id(&self) -> Result<String> {
    let group = self.create_mls_group();

    Ok(group.added_by_inbox_id().map_err(ErrorWrapper::from)?)
  }

  #[napi]
  pub async fn group_metadata(&self) -> Result<GroupMetadata> {
    let group = self.create_mls_group();

    let metadata = group.metadata().await.map_err(ErrorWrapper::from)?;

    Ok(GroupMetadata { inner: metadata })
  }

  #[napi]
  pub fn consent_state(&self) -> Result<ConsentState> {
    let group = self.create_mls_group();

    let state = group.consent_state().map_err(ErrorWrapper::from)?;

    Ok(state.into())
  }

  #[napi]
  pub fn update_consent_state(&self, state: ConsentState) -> Result<()> {
    let group = self.create_mls_group();

    group
      .update_consent_state(state.into())
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }

  #[napi]
  pub fn dm_peer_inbox_id(&self) -> Result<String> {
    let inbox_id = self.inner_group.context.inbox_id();
    let binding = self.create_mls_group();
    let dm_id = binding.dm_id.as_ref().ok_or(napi::Error::from_reason(
      "Not a DM conversation or missing DM ID",
    ))?;
    Ok(dm_id.other_inbox_id(inbox_id))
  }

  #[napi]
  pub async fn update_permission_policy(
    &self,
    permission_update_type: PermissionUpdateType,
    permission_policy_option: PermissionPolicy,
    metadata_field: Option<MetadataField>,
  ) -> Result<()> {
    let group = self.create_mls_group();

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
    let group = self.create_mls_group();
    group
      .update_conversation_message_disappearing_settings(settings.into())
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }

  #[napi]
  pub async fn remove_message_disappearing_settings(&self) -> Result<()> {
    let group = self.create_mls_group();

    group
      .remove_conversation_message_disappearing_settings()
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }

  #[napi]
  pub fn message_disappearing_settings(&self) -> Result<Option<MessageDisappearingSettings>> {
    let settings = self
      .inner_group
      .disappearing_settings()
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
        .is_some_and(|s| s.from_ns.get_i64().0 > 0 && s.in_ns.get_i64().0 > 0)
    })
  }

  #[napi]
  pub fn get_hmac_keys(&self) -> Result<HashMap<String, Vec<HmacKey>>> {
    let group = self.create_mls_group();

    let dms = self
      .inner_group
      .find_duplicate_dms()
      .map_err(ErrorWrapper::from)?;

    let mut hmac_map = HashMap::new();
    for conversation in dms {
      let id = hex::encode(&conversation.group_id);
      let keys = conversation
        .hmac_keys(-1..=1)
        .map_err(ErrorWrapper::from)?
        .into_iter()
        .map(Into::into)
        .collect::<Vec<_>>();
      hmac_map.insert(id, keys);
    }

    let keys = group
      .hmac_keys(-1..=1)
      .map_err(ErrorWrapper::from)?
      .into_iter()
      .map(Into::into)
      .collect::<Vec<_>>();

    hmac_map.insert(self.id(), keys);

    Ok(hmac_map)
  }

  #[napi]
  pub async fn debug_info(&self) -> Result<ConversationDebugInfo> {
    let group = self.create_mls_group();

    Ok(
      group
        .debug_info()
        .await
        .map(Into::into)
        .map_err(ErrorWrapper::from)?,
    )
  }

  #[napi]
  pub async fn find_duplicate_dms(&self) -> Result<Vec<Conversation>> {
    // Await the async call and handle errors
    let dms = self
      .inner_group
      .find_duplicate_dms()
      .map_err(ErrorWrapper::from)?;

    let conversations: Vec<Conversation> = dms.into_iter().map(Into::into).collect();

    Ok(conversations)
  }

  #[napi]
  pub async fn find_enriched_messages(
    &self,
    opts: Option<ListMessagesOptions>,
  ) -> Result<Vec<DecodedMessage>> {
    let opts = opts.unwrap_or_default();
    let group = self.create_mls_group();
    let messages: Vec<DecodedMessage> = group
      .find_messages_v2(&opts.into())
      .map_err(ErrorWrapper::from)?
      .into_iter()
      .map(|msg| msg.try_into())
      .collect::<Result<Vec<_>>>()?;

    Ok(messages)
  }

  #[napi]
  pub async fn get_last_read_times(&self) -> Result<HashMap<String, i64>> {
    let group = self.create_mls_group();
    let times = group.get_last_read_times().map_err(ErrorWrapper::from)?;
    Ok(times)
  }

  #[napi]
  pub async fn leave_group(&self) -> Result<()> {
    let group = self.create_mls_group();
    group.leave_group().await.map_err(ErrorWrapper::from)?;
    Ok(())
  }
}
