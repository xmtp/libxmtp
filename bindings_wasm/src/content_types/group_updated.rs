use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone)]
pub struct GroupUpdated {
  #[wasm_bindgen(js_name = "initiatedByInboxId")]
  pub initiated_by_inbox_id: String,
  #[wasm_bindgen(js_name = "addedInboxes")]
  pub added_inboxes: Vec<Inbox>,
  #[wasm_bindgen(js_name = "removedInboxes")]
  pub removed_inboxes: Vec<Inbox>,
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
