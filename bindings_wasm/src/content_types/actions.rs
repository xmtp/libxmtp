use chrono::DateTime;
use js_sys::Uint8Array;
use prost::Message;
use serde::{Deserialize, Serialize};
use wasm_bindgen::{JsError, prelude::wasm_bindgen};
use xmtp_content_types::{ContentCodec, actions::ActionsCodec};
use xmtp_proto::xmtp::mls::message_contents::EncodedContent;

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone, Serialize, Deserialize)]
pub struct Actions {
  pub id: String,
  pub description: String,
  #[wasm_bindgen(skip)]
  pub actions: Vec<Action>,
  #[wasm_bindgen(js_name = "expiresAtNs")]
  pub expires_at_ns: Option<i64>,
}

#[wasm_bindgen]
impl Actions {
  #[wasm_bindgen(constructor)]
  pub fn new(id: String, description: String, expires_at_ns: Option<i64>) -> Self {
    Self {
      id,
      description,
      actions: Vec::new(),
      expires_at_ns,
    }
  }

  #[wasm_bindgen(js_name = "getActions")]
  pub fn get_actions(&self) -> Vec<Action> {
    self.actions.clone()
  }

  #[wasm_bindgen(js_name = "setActions")]
  pub fn set_actions(&mut self, actions: Vec<Action>) {
    self.actions = actions;
  }

  #[wasm_bindgen(js_name = "addAction")]
  pub fn add_action(&mut self, action: Action) {
    self.actions.push(action);
  }
}

impl From<xmtp_content_types::actions::Actions> for Actions {
  fn from(actions: xmtp_content_types::actions::Actions) -> Self {
    Self {
      id: actions.id,
      description: actions.description,
      actions: actions.actions.into_iter().map(|a| a.into()).collect(),
      expires_at_ns: actions
        .expires_at
        .map(|dt| dt.and_utc().timestamp_nanos_opt().unwrap_or(0)),
    }
  }
}

impl From<Actions> for xmtp_content_types::actions::Actions {
  fn from(actions: Actions) -> Self {
    Self {
      id: actions.id,
      description: actions.description,
      actions: actions.actions.into_iter().map(|a| a.into()).collect(),
      expires_at: actions
        .expires_at_ns
        .map(|ns| DateTime::from_timestamp_nanos(ns).naive_utc()),
    }
  }
}

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone, Serialize, Deserialize)]
pub struct Action {
  pub id: String,
  pub label: String,
  #[wasm_bindgen(js_name = "imageUrl")]
  pub image_url: Option<String>,
  #[wasm_bindgen]
  pub style: Option<ActionStyle>,
  #[wasm_bindgen(js_name = "expiresAtNs")]
  pub expires_at_ns: Option<i64>,
}

#[wasm_bindgen]
impl Action {
  #[wasm_bindgen(constructor)]
  pub fn new(
    id: String,
    label: String,
    image_url: Option<String>,
    style: Option<ActionStyle>,
    expires_at_ns: Option<i64>,
  ) -> Self {
    Self {
      id,
      label,
      image_url,
      style,
      expires_at_ns,
    }
  }
}

impl From<xmtp_content_types::actions::Action> for Action {
  fn from(action: xmtp_content_types::actions::Action) -> Self {
    Self {
      id: action.id,
      label: action.label,
      image_url: action.image_url,
      style: action.style.map(|s| s.into()),
      expires_at_ns: action
        .expires_at
        .map(|dt| dt.and_utc().timestamp_nanos_opt().unwrap_or(0)),
    }
  }
}

impl From<Action> for xmtp_content_types::actions::Action {
  fn from(action: Action) -> Self {
    Self {
      id: action.id,
      label: action.label,
      image_url: action.image_url,
      style: action.style.map(|s| s.into()),
      expires_at: action
        .expires_at_ns
        .map(|ns| DateTime::from_timestamp_nanos(ns).naive_utc()),
    }
  }
}

#[wasm_bindgen]
#[derive(Clone, Copy, Serialize, Deserialize)]
pub enum ActionStyle {
  Primary,
  Secondary,
  Danger,
}

impl From<xmtp_content_types::actions::ActionStyle> for ActionStyle {
  fn from(style: xmtp_content_types::actions::ActionStyle) -> Self {
    match style {
      xmtp_content_types::actions::ActionStyle::Primary => ActionStyle::Primary,
      xmtp_content_types::actions::ActionStyle::Secondary => ActionStyle::Secondary,
      xmtp_content_types::actions::ActionStyle::Danger => ActionStyle::Danger,
    }
  }
}

impl From<ActionStyle> for xmtp_content_types::actions::ActionStyle {
  fn from(style: ActionStyle) -> Self {
    match style {
      ActionStyle::Primary => xmtp_content_types::actions::ActionStyle::Primary,
      ActionStyle::Secondary => xmtp_content_types::actions::ActionStyle::Secondary,
      ActionStyle::Danger => xmtp_content_types::actions::ActionStyle::Danger,
    }
  }
}

#[wasm_bindgen(js_name = "encodeActions")]
pub fn encode_actions(actions: Actions) -> Result<Uint8Array, JsError> {
  // Use ActionsCodec to encode the actions
  let encoded =
    ActionsCodec::encode(actions.into()).map_err(|e| JsError::new(&format!("{}", e)))?;

  // Encode the EncodedContent to bytes
  let mut buf = Vec::new();
  encoded
    .encode(&mut buf)
    .map_err(|e| JsError::new(&format!("{}", e)))?;

  Ok(Uint8Array::from(buf.as_slice()))
}

#[wasm_bindgen(js_name = "decodeActions")]
pub fn decode_actions(bytes: Uint8Array) -> Result<Actions, JsError> {
  // Decode bytes into EncodedContent
  let encoded_content = EncodedContent::decode(bytes.to_vec().as_slice())
    .map_err(|e| JsError::new(&format!("{}", e)))?;

  // Use ActionsCodec to decode into Actions and convert to Actions
  ActionsCodec::decode(encoded_content)
    .map(Into::into)
    .map_err(|e| JsError::new(&format!("{}", e)))
}
