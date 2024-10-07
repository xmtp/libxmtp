use std::sync::Arc;
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::{JsError, JsValue};
use xmtp_mls::client::FindGroupParams;
use xmtp_mls::groups::{GroupMetadataOptions, PreconfiguredPolicies};

use crate::messages::WasmMessage;
use crate::permissions::WasmGroupPermissionsOptions;
use crate::{groups::WasmGroup, mls_client::RustXmtpClient};

#[wasm_bindgen(getter_with_clone)]
pub struct WasmListConversationsOptions {
  pub created_after_ns: Option<i64>,
  pub created_before_ns: Option<i64>,
  pub limit: Option<i64>,
}

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone)]
pub struct WasmCreateGroupOptions {
  pub permissions: Option<WasmGroupPermissionsOptions>,
  pub group_name: Option<String>,
  pub group_image_url_square: Option<String>,
  pub group_description: Option<String>,
  pub group_pinned_frame_url: Option<String>,
}

impl WasmCreateGroupOptions {
  pub fn into_group_metadata_options(self) -> GroupMetadataOptions {
    GroupMetadataOptions {
      name: self.group_name,
      image_url_square: self.group_image_url_square,
      description: self.group_description,
      pinned_frame_url: self.group_pinned_frame_url,
    }
  }
}

#[wasm_bindgen]
pub struct WasmConversations {
  inner_client: Arc<RustXmtpClient>,
}

impl WasmConversations {
  pub fn new(inner_client: Arc<RustXmtpClient>) -> Self {
    Self { inner_client }
  }
}

#[wasm_bindgen]
impl WasmConversations {
  #[wasm_bindgen]
  pub async fn create_group(
    &self,
    account_addresses: Vec<String>,
    options: Option<WasmCreateGroupOptions>,
  ) -> Result<WasmGroup, JsError> {
    let options = match options {
      Some(options) => options,
      None => WasmCreateGroupOptions {
        permissions: None,
        group_name: None,
        group_image_url_square: None,
        group_description: None,
        group_pinned_frame_url: None,
      },
    };

    let group_permissions = match options.permissions {
      Some(WasmGroupPermissionsOptions::AllMembers) => {
        Some(PreconfiguredPolicies::AllMembers.to_policy_set())
      }
      Some(WasmGroupPermissionsOptions::AdminOnly) => {
        Some(PreconfiguredPolicies::AdminsOnly.to_policy_set())
      }
      _ => None,
    };

    let metadata_options = options.clone().into_group_metadata_options();

    let convo = if account_addresses.is_empty() {
      self
        .inner_client
        .create_group(group_permissions, metadata_options)
        .map_err(|e| JsError::new(format!("{}", e).as_str()))?
    } else {
      self
        .inner_client
        .create_group_with_members(account_addresses, group_permissions, metadata_options)
        .await
        .map_err(|e| JsError::new(format!("{}", e).as_str()))?
    };

    let out = WasmGroup::new(
      self.inner_client.clone(),
      convo.group_id,
      convo.created_at_ns,
    );

    Ok(out)
  }

  #[wasm_bindgen]
  pub fn find_group_by_id(&self, group_id: String) -> Result<WasmGroup, JsError> {
    let group_id = hex::decode(group_id).map_err(|e| JsError::new(format!("{}", e).as_str()))?;

    let group = self
      .inner_client
      .group(group_id)
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;

    Ok(WasmGroup::new(
      self.inner_client.clone(),
      group.group_id,
      group.created_at_ns,
    ))
  }

  #[wasm_bindgen]
  pub fn find_message_by_id(&self, message_id: String) -> Result<WasmMessage, JsError> {
    let message_id =
      hex::decode(message_id).map_err(|e| JsError::new(format!("{}", e).as_str()))?;

    let message = self
      .inner_client
      .message(message_id)
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;

    Ok(WasmMessage::from(message))
  }

  #[wasm_bindgen]
  pub async fn sync(&self) -> Result<(), JsError> {
    self
      .inner_client
      .sync_welcomes()
      .await
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;
    Ok(())
  }

  #[wasm_bindgen]
  pub async fn list(
    &self,
    opts: Option<WasmListConversationsOptions>,
  ) -> Result<js_sys::Array, JsError> {
    let opts = match opts {
      Some(options) => options,
      None => WasmListConversationsOptions {
        created_after_ns: None,
        created_before_ns: None,
        limit: None,
      },
    };
    let convo_list: js_sys::Array = self
      .inner_client
      .find_groups(FindGroupParams {
        created_after_ns: opts.created_after_ns,
        created_before_ns: opts.created_before_ns,
        limit: opts.limit,
        ..FindGroupParams::default()
      })
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?
      .into_iter()
      .map(|group| {
        JsValue::from(WasmGroup::new(
          self.inner_client.clone(),
          group.group_id,
          group.created_at_ns,
        ))
      })
      .collect();

    Ok(convo_list)
  }
}
