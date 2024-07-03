use std::{ops::Deref, sync::Arc};

use napi::{
  bindgen_prelude::{Error, Result, Uint8Array},
  threadsafe_function::{ErrorStrategy, ThreadsafeFunction, ThreadsafeFunctionCallMode},
  JsFunction,
};
use xmtp_cryptography::signature::ed25519_public_key_to_address;
use xmtp_mls::groups::{
  group_metadata::{ConversationType, GroupMetadata},
  group_permissions::GroupMutablePermissions,
  members::PermissionLevel,
  MlsGroup, PreconfiguredPolicies, UpdateAdminListType,
};
use xmtp_proto::xmtp::mls::message_contents::EncodedContent;

use crate::{
  encoded_content::NapiEncodedContent,
  messages::{NapiListMessagesOptions, NapiMessage},
  mls_client::RustXmtpClient,
  streams::NapiStreamCloser,
};

use prost::Message;

use napi_derive::napi;

#[napi]
pub enum GroupPermissions {
  EveryoneIsAdmin,
  GroupCreatorIsAdmin,
}

impl From<PreconfiguredPolicies> for GroupPermissions {
  fn from(policy: PreconfiguredPolicies) -> Self {
    match policy {
      PreconfiguredPolicies::AllMembers => GroupPermissions::EveryoneIsAdmin,
      PreconfiguredPolicies::AdminsOnly => GroupPermissions::GroupCreatorIsAdmin,
    }
  }
}

impl From<GroupPermissions> for PreconfiguredPolicies {
  fn from(permissions: GroupPermissions) -> Self {
    match permissions {
      GroupPermissions::EveryoneIsAdmin => PreconfiguredPolicies::AllMembers,
      GroupPermissions::GroupCreatorIsAdmin => PreconfiguredPolicies::AdminsOnly,
    }
  }
}

#[napi]
pub struct NapiGroupMetadata {
  inner: GroupMetadata,
}

#[napi]
impl NapiGroupMetadata {
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
pub enum NapiPermissionLevel {
  Member,
  Admin,
  SuperAdmin,
}

#[napi]
pub struct NapiGroupMember {
  pub inbox_id: String,
  pub account_addresses: Vec<String>,
  pub installation_ids: Vec<String>,
  pub permission_level: NapiPermissionLevel,
}

#[napi]
pub struct NapiGroupPermissions {
  inner: GroupMutablePermissions,
}

#[napi]
impl NapiGroupPermissions {
  #[napi]
  pub fn policy_type(&self) -> Result<GroupPermissions> {
    Ok(
      self
        .inner
        .preconfigured_policy()
        .map_err(|e| Error::from_reason(format!("{}", e)))?
        .into(),
    )
  }
}

#[derive(Debug)]
#[napi]
pub struct NapiGroup {
  inner_client: Arc<RustXmtpClient>,
  group_id: Vec<u8>,
  created_at_ns: i64,
}

#[napi]
impl NapiGroup {
  pub fn new(inner_client: Arc<RustXmtpClient>, group_id: Vec<u8>, created_at_ns: i64) -> Self {
    Self {
      inner_client,
      group_id,
      created_at_ns,
    }
  }

  #[napi]
  pub fn id(&self) -> String {
    hex::encode(self.group_id.clone())
  }

  #[napi]
  pub async fn send(&self, encoded_content: NapiEncodedContent) -> Result<String> {
    let encoded_content: EncodedContent = encoded_content.into();
    let group = MlsGroup::new(
      self.inner_client.context().clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );

    let message_id = group
      .send_message(
        encoded_content.encode_to_vec().as_slice(),
        &self.inner_client,
      )
      .await
      .map_err(|e| Error::from_reason(format!("{}", e)))?;
    Ok(hex::encode(message_id.clone()))
  }

  #[napi]
  pub async fn sync(&self) -> Result<()> {
    let group = MlsGroup::new(
      self.inner_client.context().clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );

    group
      .sync(&self.inner_client)
      .await
      .map_err(|e| Error::from_reason(format!("{}", e)))?;

    Ok(())
  }

  #[napi]
  pub fn find_messages(&self, opts: Option<NapiListMessagesOptions>) -> Result<Vec<NapiMessage>> {
    let opts = match opts {
      Some(options) => options,
      None => NapiListMessagesOptions {
        sent_before_ns: None,
        sent_after_ns: None,
        limit: None,
        delivery_status: None,
      },
    };

    let group = MlsGroup::new(
      self.inner_client.context().clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );

    let delivery_status = opts.delivery_status.map(|status| status.into());

    let messages: Vec<NapiMessage> = group
      .find_messages(
        None,
        opts.sent_before_ns,
        opts.sent_after_ns,
        delivery_status,
        opts.limit,
      )
      .map_err(|e| Error::from_reason(format!("{}", e)))?
      .into_iter()
      .map(|msg| msg.into())
      .collect();

    Ok(messages)
  }

  #[napi]
  pub async fn process_streamed_group_message(
    &self,
    envelope_bytes: Uint8Array,
  ) -> Result<NapiMessage> {
    let group = MlsGroup::new(
      self.inner_client.context().clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );
    let envelope_bytes: Vec<u8> = envelope_bytes.deref().to_vec();
    let message = group
      .process_streamed_group_message(envelope_bytes, self.inner_client.clone())
      .await
      .map_err(|e| Error::from_reason(format!("{}", e)))?;

    Ok(message.into())
  }

  #[napi]
  pub fn list_members(&self) -> Result<Vec<NapiGroupMember>> {
    let group = MlsGroup::new(
      self.inner_client.context().clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );

    let members: Vec<NapiGroupMember> = group
      .members()
      .map_err(|e| Error::from_reason(format!("{}", e)))?
      .into_iter()
      .map(|member| NapiGroupMember {
        inbox_id: member.inbox_id,
        account_addresses: member.account_addresses,
        installation_ids: member
          .installation_ids
          .into_iter()
          .map(|id| ed25519_public_key_to_address(id.as_slice()))
          .collect(),
        permission_level: match member.permission_level {
          PermissionLevel::Member => NapiPermissionLevel::Member,
          PermissionLevel::Admin => NapiPermissionLevel::Admin,
          PermissionLevel::SuperAdmin => NapiPermissionLevel::SuperAdmin,
        },
      })
      .collect();

    Ok(members)
  }

  #[napi]
  pub fn admin_list(&self) -> Result<Vec<String>> {
    let group = MlsGroup::new(
      self.inner_client.context().clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );

    let admin_list = group
      .admin_list()
      .map_err(|e| Error::from_reason(format!("{}", e)))?;

    Ok(admin_list)
  }

  #[napi]
  pub fn super_admin_list(&self) -> Result<Vec<String>> {
    let group = MlsGroup::new(
      self.inner_client.context().clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );

    let super_admin_list = group
      .super_admin_list()
      .map_err(|e| Error::from_reason(format!("{}", e)))?;

    Ok(super_admin_list)
  }

  #[napi]
  pub fn is_admin(&self, inbox_id: String) -> Result<bool> {
    let admin_list = self
      .admin_list()
      .map_err(|e| Error::from_reason(format!("{}", e)))?;
    Ok(admin_list.contains(&inbox_id))
  }

  #[napi]
  pub fn is_super_admin(&self, inbox_id: String) -> Result<bool> {
    let super_admin_list = self
      .super_admin_list()
      .map_err(|e| Error::from_reason(format!("{}", e)))?;
    Ok(super_admin_list.contains(&inbox_id))
  }

  #[napi]
  pub async fn add_members(&self, account_addresses: Vec<String>) -> Result<()> {
    let group = MlsGroup::new(
      self.inner_client.context().clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );

    group
      .add_members(&self.inner_client, account_addresses)
      .await
      .map_err(|e| Error::from_reason(format!("{}", e)))?;

    Ok(())
  }

  #[napi]
  pub async fn add_admin(&self, inbox_id: String) -> Result<()> {
    let group = MlsGroup::new(
      self.inner_client.context().clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );
    group
      .update_admin_list(&self.inner_client, UpdateAdminListType::Add, inbox_id)
      .await
      .map_err(|e| Error::from_reason(format!("{}", e)))?;

    Ok(())
  }

  #[napi]
  pub async fn remove_admin(&self, inbox_id: String) -> Result<()> {
    let group = MlsGroup::new(
      self.inner_client.context().clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );
    group
      .update_admin_list(&self.inner_client, UpdateAdminListType::Remove, inbox_id)
      .await
      .map_err(|e| Error::from_reason(format!("{}", e)))?;

    Ok(())
  }

  #[napi]
  pub async fn add_super_admin(&self, inbox_id: String) -> Result<()> {
    let group = MlsGroup::new(
      self.inner_client.context().clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );
    group
      .update_admin_list(&self.inner_client, UpdateAdminListType::AddSuper, inbox_id)
      .await
      .map_err(|e| Error::from_reason(format!("{}", e)))?;

    Ok(())
  }

  #[napi]
  pub async fn remove_super_admin(&self, inbox_id: String) -> Result<()> {
    let group = MlsGroup::new(
      self.inner_client.context().clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );
    group
      .update_admin_list(
        &self.inner_client,
        UpdateAdminListType::RemoveSuper,
        inbox_id,
      )
      .await
      .map_err(|e| Error::from_reason(format!("{}", e)))?;

    Ok(())
  }

  #[napi]
  pub fn group_permissions(&self) -> Result<NapiGroupPermissions> {
    let group = MlsGroup::new(
      self.inner_client.context().clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );

    let permissions = group
      .permissions()
      .map_err(|e| Error::from_reason(format!("{}", e)))?;

    Ok(NapiGroupPermissions { inner: permissions })
  }

  #[napi]
  pub async fn add_members_by_inbox_id(&self, inbox_ids: Vec<String>) -> Result<()> {
    let group = MlsGroup::new(
      self.inner_client.context().clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );

    group
      .add_members_by_inbox_id(&self.inner_client, inbox_ids)
      .await
      .map_err(|e| Error::from_reason(format!("{}", e)))?;

    Ok(())
  }

  #[napi]
  pub async fn remove_members(&self, account_addresses: Vec<String>) -> Result<()> {
    let group = MlsGroup::new(
      self.inner_client.context().clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );

    group
      .remove_members(&self.inner_client, account_addresses)
      .await
      .map_err(|e| Error::from_reason(format!("{}", e)))?;

    Ok(())
  }

  #[napi]
  pub async fn remove_members_by_inbox_id(&self, inbox_ids: Vec<String>) -> Result<()> {
    let group = MlsGroup::new(
      self.inner_client.context().clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );

    group
      .remove_members_by_inbox_id(&self.inner_client, inbox_ids)
      .await
      .map_err(|e| Error::from_reason(format!("{}", e)))?;

    Ok(())
  }

  #[napi]
  pub async fn update_group_name(&self, group_name: String) -> Result<()> {
    let group = MlsGroup::new(
      self.inner_client.context().clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );

    group
      .update_group_name(&self.inner_client, group_name)
      .await
      .map_err(|e| Error::from_reason(format!("{}", e)))?;

    Ok(())
  }

  #[napi]
  pub fn group_name(&self) -> Result<String> {
    let group = MlsGroup::new(
      self.inner_client.context().clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );

    let group_name = group
      .group_name()
      .map_err(|e| Error::from_reason(format!("{}", e)))?;

    Ok(group_name)
  }

  #[napi]
  pub async fn update_group_image_url_square(&self, group_image_url_square: String) -> Result<()> {
    let group = MlsGroup::new(
      self.inner_client.context().clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );

    group
      .update_group_image_url_square(&self.inner_client, group_image_url_square)
      .await
      .map_err(|e| Error::from_reason(format!("{}", e)))?;

    Ok(())
  }

  #[napi]
  pub fn group_image_url_square(&self) -> Result<String> {
    let group = MlsGroup::new(
      self.inner_client.context().clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );

    let group_image_url_square = group
      .group_image_url_square()
      .map_err(|e| Error::from_reason(format!("{}", e)))?;

    Ok(group_image_url_square)
  }

  #[napi]
  pub async fn update_group_description(&self, group_description: String) -> Result<()> {
    let group = MlsGroup::new(
      self.inner_client.context().clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );

    group
      .update_group_description(&self.inner_client, group_description)
      .await
      .map_err(|e| Error::from_reason(format!("{}", e)))?;

    Ok(())
  }

  #[napi]
  pub fn group_description(&self) -> Result<String> {
    let group = MlsGroup::new(
      self.inner_client.context().clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );

    let group_description = group
      .group_description()
      .map_err(|e| Error::from_reason(format!("{}", e)))?;

    Ok(group_description)
  }

  #[napi]
  pub async fn update_group_pinned_frame_url(&self, pinned_frame_url: String) -> Result<()> {
    let group = MlsGroup::new(
      self.inner_client.context().clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );

    group
      .update_group_pinned_frame_url(&self.inner_client, pinned_frame_url)
      .await
      .map_err(|e| Error::from_reason(format!("{}", e)))?;

    Ok(())
  }

  #[napi]
  pub fn group_pinned_frame_url(&self) -> Result<String> {
    let group = MlsGroup::new(
      self.inner_client.context().clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );

    let group_pinned_frame_url = group
      .group_pinned_frame_url()
      .map_err(|e| Error::from_reason(format!("{}", e)))?;

    Ok(group_pinned_frame_url)
  }

  #[napi(ts_args_type = "callback: (err: null | Error, result: NapiMessage) => void")]
  pub fn stream(&self, callback: JsFunction) -> Result<NapiStreamCloser> {
    let tsfn: ThreadsafeFunction<NapiMessage, ErrorStrategy::CalleeHandled> =
      callback.create_threadsafe_function(0, |ctx| Ok(vec![ctx.value]))?;
    let stream_closer = MlsGroup::stream_with_callback(
      self.inner_client.clone(),
      self.group_id.clone(),
      self.created_at_ns,
      move |message| {
        tsfn.call(Ok(message.into()), ThreadsafeFunctionCallMode::Blocking);
      },
    )
    .map_err(|e| Error::from_reason(format!("{}", e)))?;

    Ok(NapiStreamCloser::new(
      stream_closer.close_fn,
      stream_closer.is_closed_atomic,
    ))
  }

  #[napi]
  pub fn created_at_ns(&self) -> i64 {
    self.created_at_ns
  }

  #[napi]
  pub fn is_active(&self) -> Result<bool> {
    let group = MlsGroup::new(
      self.inner_client.context().clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );

    group
      .is_active()
      .map_err(|e| Error::from_reason(format!("{}", e)))
  }

  #[napi]
  pub fn added_by_inbox_id(&self) -> Result<String> {
    let group = MlsGroup::new(
      self.inner_client.context().clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );

    group
      .added_by_inbox_id()
      .map_err(|e| Error::from_reason(format!("{}", e)))
  }

  #[napi]
  pub fn group_metadata(&self) -> Result<NapiGroupMetadata> {
    let group = MlsGroup::new(
      self.inner_client.context().clone(),
      self.group_id.clone(),
      self.created_at_ns,
    );

    let metadata = group
      .metadata()
      .map_err(|e| Error::from_reason(format!("{}", e)))?;
    Ok(NapiGroupMetadata { inner: metadata })
  }
}
