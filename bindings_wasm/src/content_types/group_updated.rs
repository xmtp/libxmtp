use js_sys::Uint8Array;
use prost::Message;
use wasm_bindgen::{JsError, prelude::wasm_bindgen};
use xmtp_content_types::ContentCodec;
use xmtp_content_types::group_updated::GroupUpdatedCodec;
use xmtp_proto::xmtp::mls::message_contents::EncodedContent;

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone)]
pub struct GroupUpdated {
  #[wasm_bindgen(js_name = "initiatedByInboxId")]
  pub initiated_by_inbox_id: String,
  #[wasm_bindgen(js_name = "addedInboxes")]
  pub added_inboxes: Vec<Inbox>,
  #[wasm_bindgen(js_name = "removedInboxes")]
  pub removed_inboxes: Vec<Inbox>,
  #[wasm_bindgen(js_name = "leftInboxes")]
  pub left_inboxes: Vec<Inbox>,
  #[wasm_bindgen(js_name = "metadataFieldChanges")]
  pub metadata_field_changes: Vec<MetadataFieldChange>,
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
      left_inboxes: updated.left_inboxes.into_iter().map(|i| i.into()).collect(),
      metadata_field_changes: updated
        .metadata_field_changes
        .into_iter()
        .map(|c| c.into())
        .collect(),
    }
  }
}

impl From<GroupUpdated> for xmtp_proto::xmtp::mls::message_contents::GroupUpdated {
  fn from(updated: GroupUpdated) -> Self {
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
      left_inboxes: updated.left_inboxes.into_iter().map(|i| i.into()).collect(),
      metadata_field_changes: updated
        .metadata_field_changes
        .into_iter()
        .map(|c| c.into())
        .collect(),
    }
  }
}

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone)]
pub struct Inbox {
  #[wasm_bindgen(js_name = "inboxId")]
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

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone)]
pub struct MetadataFieldChange {
  #[wasm_bindgen(js_name = "fieldName")]
  pub field_name: String,
  #[wasm_bindgen(js_name = "oldValue")]
  pub old_value: Option<String>,
  #[wasm_bindgen(js_name = "newValue")]
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

#[wasm_bindgen(js_name = "encodeGroupUpdated")]
pub fn encode_group_updated(
  #[wasm_bindgen(js_name = "groupUpdated")] group_updated: GroupUpdated,
) -> Result<Uint8Array, JsError> {
  // Use GroupUpdatedCodec to encode the GroupUpdated
  let encoded =
    GroupUpdatedCodec::encode(group_updated.into()).map_err(|e| JsError::new(&format!("{}", e)))?;
  let mut buf = Vec::new();

  // Encode the EncodedContent to bytes
  encoded
    .encode(&mut buf)
    .map_err(|e| JsError::new(&format!("{}", e)))?;

  Ok(Uint8Array::from(buf.as_slice()))
}

#[wasm_bindgen(js_name = "decodeGroupUpdated")]
pub fn decode_group_updated(bytes: Uint8Array) -> Result<GroupUpdated, JsError> {
  // Decode bytes into EncodedContent
  let encoded = EncodedContent::decode(bytes.to_vec().as_slice())
    .map_err(|e| JsError::new(&format!("{}", e)))?;

  // Use GroupUpdatedCodec to decode into GroupUpdated and convert to GroupUpdated
  GroupUpdatedCodec::decode(encoded)
    .map(Into::into)
    .map_err(|e| JsError::new(&format!("{}", e)))
}
