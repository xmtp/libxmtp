use napi::bindgen_prelude::Result;
use napi_derive::napi;
use xmtp_content_types::{ContentCodec, group_updated::GroupUpdatedCodec as XmtpGroupUpdatedCodec};

use crate::{
  ErrorWrapper,
  encoded_content::{ContentTypeId, EncodedContent},
};

#[derive(Clone)]
#[napi(object)]
pub struct GroupUpdated {
  pub initiated_by_inbox_id: String,
  pub added_inboxes: Vec<Inbox>,
  pub removed_inboxes: Vec<Inbox>,
  pub left_inboxes: Vec<Inbox>,
  pub metadata_field_changes: Vec<MetadataFieldChange>,
  pub added_admin_inboxes: Vec<Inbox>,
  pub removed_admin_inboxes: Vec<Inbox>,
  pub added_super_admin_inboxes: Vec<Inbox>,
  pub removed_super_admin_inboxes: Vec<Inbox>,
}

impl From<xmtp_proto::xmtp::mls::message_contents::GroupUpdated> for GroupUpdated {
  fn from(updated: xmtp_proto::xmtp::mls::message_contents::GroupUpdated) -> Self {
    Self {
      initiated_by_inbox_id: updated.initiated_by_inbox_id,
      added_inboxes: updated
        .added_inboxes
        .into_iter()
        .map(|i| i.into())
        .collect(),
      removed_inboxes: updated
        .removed_inboxes
        .into_iter()
        .map(|i| i.into())
        .collect(),
      metadata_field_changes: updated
        .metadata_field_changes
        .into_iter()
        .map(|c| c.into())
        .collect(),
      left_inboxes: updated.left_inboxes.into_iter().map(|i| i.into()).collect(),
      added_admin_inboxes: updated
        .added_admin_inboxes
        .into_iter()
        .map(|i| i.into())
        .collect(),
      removed_admin_inboxes: updated
        .removed_admin_inboxes
        .into_iter()
        .map(|i| i.into())
        .collect(),
      added_super_admin_inboxes: updated
        .added_super_admin_inboxes
        .into_iter()
        .map(|i| i.into())
        .collect(),
      removed_super_admin_inboxes: updated
        .removed_super_admin_inboxes
        .into_iter()
        .map(|i| i.into())
        .collect(),
    }
  }
}

#[derive(Clone)]
#[napi(object)]
pub struct Inbox {
  pub inbox_id: String,
}

impl From<xmtp_proto::xmtp::mls::message_contents::group_updated::Inbox> for Inbox {
  fn from(inbox: xmtp_proto::xmtp::mls::message_contents::group_updated::Inbox) -> Self {
    Self {
      inbox_id: inbox.inbox_id,
    }
  }
}

impl From<Inbox> for xmtp_proto::xmtp::mls::message_contents::group_updated::Inbox {
  fn from(inbox: Inbox) -> Self {
    Self {
      inbox_id: inbox.inbox_id,
    }
  }
}

#[derive(Clone)]
#[napi(object)]
pub struct MetadataFieldChange {
  pub field_name: String,
  pub old_value: Option<String>,
  pub new_value: Option<String>,
}

impl From<xmtp_proto::xmtp::mls::message_contents::group_updated::MetadataFieldChange>
  for MetadataFieldChange
{
  fn from(
    change: xmtp_proto::xmtp::mls::message_contents::group_updated::MetadataFieldChange,
  ) -> Self {
    Self {
      field_name: change.field_name,
      old_value: change.old_value,
      new_value: change.new_value,
    }
  }
}

impl From<MetadataFieldChange>
  for xmtp_proto::xmtp::mls::message_contents::group_updated::MetadataFieldChange
{
  fn from(change: MetadataFieldChange) -> Self {
    Self {
      field_name: change.field_name,
      old_value: change.old_value,
      new_value: change.new_value,
    }
  }
}

#[napi]
pub struct GroupUpdatedCodec {}

#[napi]
impl GroupUpdatedCodec {
  #[napi]
  pub fn content_type() -> ContentTypeId {
    XmtpGroupUpdatedCodec::content_type().into()
  }

  #[napi]
  pub fn decode(encoded_content: EncodedContent) -> Result<GroupUpdated> {
    Ok(
      XmtpGroupUpdatedCodec::decode(encoded_content.into())
        .map(Into::into)
        .map_err(ErrorWrapper::from)?,
    )
  }

  #[napi]
  pub fn should_push() -> bool {
    XmtpGroupUpdatedCodec::should_push()
  }
}
