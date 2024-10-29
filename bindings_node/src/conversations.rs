use std::ops::Deref;
use std::sync::Arc;
use std::vec;

use napi::bindgen_prelude::{Error, Result, Uint8Array};
use napi::threadsafe_function::{ErrorStrategy, ThreadsafeFunction, ThreadsafeFunctionCallMode};
use napi::JsFunction;
use napi_derive::napi;
use xmtp_mls::groups::group_metadata::ConversationType;
use xmtp_mls::groups::{GroupMetadataOptions, PreconfiguredPolicies};
use xmtp_mls::storage::group::GroupQueryArgs;

use crate::messages::NapiMessage;
use crate::permissions::NapiGroupPermissionsOptions;
use crate::ErrorWrapper;
use crate::{groups::NapiGroup, mls_client::RustXmtpClient, streams::NapiStreamCloser};

#[napi(object)]
pub struct NapiListConversationsOptions {
  pub created_after_ns: Option<i64>,
  pub created_before_ns: Option<i64>,
  pub limit: Option<i64>,
}

impl From<NapiListConversationsOptions> for GroupQueryArgs {
  fn from(opts: NapiListConversationsOptions) -> GroupQueryArgs {
    GroupQueryArgs::default()
      .maybe_created_after_ns(opts.created_after_ns)
      .maybe_created_before_ns(opts.created_before_ns)
      .maybe_limit(opts.limit)
  }
}

#[napi(object)]
#[derive(Clone)]
pub struct NapiCreateGroupOptions {
  pub permissions: Option<NapiGroupPermissionsOptions>,
  pub group_name: Option<String>,
  pub group_image_url_square: Option<String>,
  pub group_description: Option<String>,
  pub group_pinned_frame_url: Option<String>,
}

impl NapiCreateGroupOptions {
  pub fn into_group_metadata_options(self) -> GroupMetadataOptions {
    GroupMetadataOptions {
      name: self.group_name,
      image_url_square: self.group_image_url_square,
      description: self.group_description,
      pinned_frame_url: self.group_pinned_frame_url,
    }
  }
}

#[napi]
pub struct NapiConversations {
  inner_client: Arc<RustXmtpClient>,
}

#[napi]
impl NapiConversations {
  pub fn new(inner_client: Arc<RustXmtpClient>) -> Self {
    Self { inner_client }
  }

  #[napi]
  pub async fn create_group(
    &self,
    account_addresses: Vec<String>,
    options: Option<NapiCreateGroupOptions>,
  ) -> Result<NapiGroup> {
    let options = match options {
      Some(options) => options,
      None => NapiCreateGroupOptions {
        permissions: None,
        group_name: None,
        group_image_url_square: None,
        group_description: None,
        group_pinned_frame_url: None,
      },
    };

    let group_permissions = match options.permissions {
      Some(NapiGroupPermissionsOptions::AllMembers) => {
        Some(PreconfiguredPolicies::AllMembers.to_policy_set())
      }
      Some(NapiGroupPermissionsOptions::AdminOnly) => {
        Some(PreconfiguredPolicies::AdminsOnly.to_policy_set())
      }
      _ => None,
    };

    let metadata_options = options.clone().into_group_metadata_options();

    let convo = if account_addresses.is_empty() {
      self
        .inner_client
        .create_group(group_permissions, metadata_options)
        .map_err(|e| Error::from_reason(format!("ClientError: {}", e)))?
    } else {
      self
        .inner_client
        .create_group_with_members(account_addresses, group_permissions, metadata_options)
        .await
        .map_err(|e| Error::from_reason(format!("ClientError: {}", e)))?
    };

    Ok(convo.into())
  }

  #[napi]
  pub fn find_group_by_id(&self, group_id: String) -> Result<NapiGroup> {
    let group_id = hex::decode(group_id).map_err(ErrorWrapper::from)?;

    let group = self
      .inner_client
      .group(group_id)
      .map_err(ErrorWrapper::from)?;

    Ok(group.into())
  }

  #[napi]
  pub fn find_dm_by_target_inbox_id(&self, target_inbox_id: String) -> Result<NapiGroup> {
    let convo = self
      .inner_client
      .dm_group_from_target_inbox(target_inbox_id)
      .map_err(ErrorWrapper::from)?;

    Ok(convo.into())
  }

  #[napi]
  pub fn find_message_by_id(&self, message_id: String) -> Result<NapiMessage> {
    let message_id = hex::decode(message_id).map_err(ErrorWrapper::from)?;

    let message = self
      .inner_client
      .message(message_id)
      .map_err(ErrorWrapper::from)?;

    Ok(NapiMessage::from(message))
  }

  #[napi]
  pub async fn process_streamed_welcome_message(
    &self,
    envelope_bytes: Uint8Array,
  ) -> Result<NapiGroup> {
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
  pub async fn list(&self, opts: Option<NapiListConversationsOptions>) -> Result<Vec<NapiGroup>> {
    let opts = match opts {
      Some(options) => options,
      None => NapiListConversationsOptions {
        created_after_ns: None,
        created_before_ns: None,
        limit: None,
      },
    };
    let convo_list: Vec<NapiGroup> = self
      .inner_client
      .find_groups(opts.into())
      .map_err(ErrorWrapper::from)?
      .into_iter()
      .map(NapiGroup::from)
      .collect();

    Ok(convo_list)
  }

  #[napi(ts_args_type = "callback: (err: null | Error, result: NapiGroup) => void")]
  pub fn stream(&self, callback: JsFunction) -> Result<NapiStreamCloser> {
    let tsfn: ThreadsafeFunction<NapiGroup, ErrorStrategy::CalleeHandled> =
      callback.create_threadsafe_function(0, |ctx| Ok(vec![ctx.value]))?;
    let stream_closer = RustXmtpClient::stream_conversations_with_callback(
      self.inner_client.clone(),
      Some(ConversationType::Group),
      move |convo| {
        tsfn.call(
          convo
            .map(NapiGroup::from)
            .map_err(ErrorWrapper::from)
            .map_err(Error::from),
          ThreadsafeFunctionCallMode::Blocking,
        );
      },
    );

    Ok(NapiStreamCloser::new(stream_closer))
  }

  #[napi(ts_args_type = "callback: (err: null | Error, result: NapiMessage) => void")]
  pub fn stream_all_messages(&self, callback: JsFunction) -> Result<NapiStreamCloser> {
    let tsfn: ThreadsafeFunction<NapiMessage, ErrorStrategy::CalleeHandled> =
      callback.create_threadsafe_function(0, |ctx| Ok(vec![ctx.value]))?;
    let stream_closer = RustXmtpClient::stream_all_messages_with_callback(
      self.inner_client.clone(),
      Some(ConversationType::Group),
      move |message| {
        tsfn.call(
          message
            .map(Into::into)
            .map_err(ErrorWrapper::from)
            .map_err(Error::from),
          ThreadsafeFunctionCallMode::Blocking,
        );
      },
    );

    Ok(NapiStreamCloser::new(stream_closer))
  }
}
