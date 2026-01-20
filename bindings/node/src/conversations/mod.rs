use crate::ErrorWrapper;
use crate::client::RustXmtpClient;
use crate::consent_state::ConsentState;
use crate::conversation::Conversation;
use crate::messages::Message;
use napi::bindgen_prelude::{BigInt, Result};
use napi_derive::napi;
use std::sync::Arc;
use xmtp_db::consent_record::ConsentState as XmtpConsentState;
use xmtp_db::group::GroupQueryArgs;
use xmtp_db::group::{ConversationType as XmtpConversationType, GroupQueryOrderBy};

mod dm;
mod group;
mod hmac_key;
mod messages;
mod streams;

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
pub struct GroupSyncSummary {
  pub num_eligible: u32,
  pub num_synced: u32,
}

impl From<xmtp_mls::groups::welcome_sync::GroupSyncSummary> for GroupSyncSummary {
  fn from(summary: xmtp_mls::groups::welcome_sync::GroupSyncSummary) -> Self {
    Self {
      num_eligible: summary.num_eligible as u32,
      num_synced: summary.num_synced as u32,
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
  pub fn find_group_by_id(&self, group_id: String) -> Result<Conversation> {
    let group_id = hex::decode(group_id).map_err(ErrorWrapper::from)?;

    let group = self
      .inner_client
      .stitched_group(&group_id)
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
  ) -> Result<GroupSyncSummary> {
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
  pub async fn sync_preferences(&self) -> Result<GroupSyncSummary> {
    let inner = self.inner_client.as_ref();

    let summary = inner
      .sync_all_welcomes_and_history_sync_groups()
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
}
