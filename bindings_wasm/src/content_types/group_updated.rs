use crate::encoded_content::{ContentTypeId, EncodedContent};
use serde::{Deserialize, Serialize};
use tsify::Tsify;
use wasm_bindgen::{JsError, prelude::wasm_bindgen};
use xmtp_content_types::ContentCodec;
use xmtp_content_types::group_updated::GroupUpdatedCodec as XmtpGroupUpdatedCodec;

#[derive(Clone, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
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
      added_inboxes: updated.added_inboxes.into_iter().map(Into::into).collect(),
      removed_inboxes: updated
        .removed_inboxes
        .into_iter()
        .map(Into::into)
        .collect(),
      left_inboxes: updated.left_inboxes.into_iter().map(Into::into).collect(),
      metadata_field_changes: updated
        .metadata_field_changes
        .into_iter()
        .map(|c| c.into())
        .collect(),
      added_admin_inboxes: updated
        .added_admin_inboxes
        .into_iter()
        .map(Into::into)
        .collect(),
      removed_admin_inboxes: updated
        .removed_admin_inboxes
        .into_iter()
        .map(Into::into)
        .collect(),
      added_super_admin_inboxes: updated
        .added_super_admin_inboxes
        .into_iter()
        .map(Into::into)
        .collect(),
      removed_super_admin_inboxes: updated
        .removed_super_admin_inboxes
        .into_iter()
        .map(Into::into)
        .collect(),
    }
  }
}

impl From<GroupUpdated> for xmtp_proto::xmtp::mls::message_contents::GroupUpdated {
  fn from(updated: GroupUpdated) -> Self {
    Self {
      initiated_by_inbox_id: updated.initiated_by_inbox_id,
      added_inboxes: updated.added_inboxes.into_iter().map(Into::into).collect(),
      removed_inboxes: updated
        .removed_inboxes
        .into_iter()
        .map(Into::into)
        .collect(),
      left_inboxes: updated.left_inboxes.into_iter().map(Into::into).collect(),
      metadata_field_changes: updated
        .metadata_field_changes
        .into_iter()
        .map(|c| c.into())
        .collect(),
      added_admin_inboxes: updated
        .added_admin_inboxes
        .into_iter()
        .map(Into::into)
        .collect(),
      removed_admin_inboxes: updated
        .removed_admin_inboxes
        .into_iter()
        .map(Into::into)
        .collect(),
      added_super_admin_inboxes: updated
        .added_super_admin_inboxes
        .into_iter()
        .map(Into::into)
        .collect(),
      removed_super_admin_inboxes: updated
        .removed_super_admin_inboxes
        .into_iter()
        .map(Into::into)
        .collect(),
    }
  }
}

#[derive(Clone, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
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

#[derive(Clone, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
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

#[wasm_bindgen]
pub struct GroupUpdatedCodec;

#[wasm_bindgen]
impl GroupUpdatedCodec {
  #[wasm_bindgen(js_name = "contentType")]
  pub fn content_type() -> ContentTypeId {
    XmtpGroupUpdatedCodec::content_type().into()
  }

  #[wasm_bindgen]
  pub fn decode(encoded_content: EncodedContent) -> Result<GroupUpdated, JsError> {
    XmtpGroupUpdatedCodec::decode(encoded_content.into())
      .map(Into::into)
      .map_err(|e| JsError::new(&format!("{}", e)))
  }

  #[wasm_bindgen(js_name = "shouldPush")]
  pub fn should_push() -> bool {
    XmtpGroupUpdatedCodec::should_push()
  }
}
