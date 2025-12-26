use bindings_wasm_macros::wasm_bindgen_numbered_enum;
use chrono::DateTime;
use serde::{Deserialize, Serialize};
use tsify::Tsify;
use wasm_bindgen::JsError;
use wasm_bindgen::prelude::wasm_bindgen;
use xmtp_content_types::ContentCodec;
use xmtp_content_types::actions::ActionsCodec;

use crate::encoded_content::{ContentTypeId, EncodedContent};

#[derive(Clone, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi, large_number_types_as_bigints)]
#[serde(rename_all = "camelCase")]
pub struct Actions {
  pub id: String,
  pub description: String,
  pub actions: Vec<Action>,
  #[tsify(optional)]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub expires_at_ns: Option<i64>,
}

impl TryFrom<xmtp_content_types::actions::Actions> for Actions {
  type Error = JsError;

  fn try_from(actions: xmtp_content_types::actions::Actions) -> Result<Self, Self::Error> {
    let actions_id = actions.id.clone();
    let expires_at_ns = match actions.expires_at {
      Some(dt) => {
        let ns_opt = dt.and_utc().timestamp_nanos_opt();
        if ns_opt.is_none() {
          return Err(JsError::new(&format!(
            "Actions '{}' expiration timestamp is out of valid range for conversion to nanoseconds",
            actions_id
          )));
        }
        ns_opt
      }
      None => None,
    };

    let converted_actions: Result<Vec<_>, _> =
      actions.actions.into_iter().map(|a| a.try_into()).collect();

    Ok(Self {
      id: actions.id,
      description: actions.description,
      actions: converted_actions?,
      expires_at_ns,
    })
  }
}

impl From<Actions> for xmtp_content_types::actions::Actions {
  fn from(actions: Actions) -> Self {
    let expires_at = match actions.expires_at_ns {
      Some(ns) => {
        let dt = DateTime::from_timestamp_nanos(ns).naive_utc();
        Some(dt)
      }
      None => None,
    };

    Self {
      id: actions.id,
      description: actions.description,
      actions: actions.actions.into_iter().map(|a| a.into()).collect(),
      expires_at,
    }
  }
}

#[derive(Clone, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi, large_number_types_as_bigints)]
#[serde(rename_all = "camelCase")]
pub struct Action {
  pub id: String,
  pub label: String,
  #[tsify(optional)]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub image_url: Option<String>,
  #[tsify(optional)]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub style: Option<ActionStyle>,
  #[tsify(optional)]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub expires_at_ns: Option<i64>,
}

impl TryFrom<xmtp_content_types::actions::Action> for Action {
  type Error = JsError;

  fn try_from(action: xmtp_content_types::actions::Action) -> Result<Self, Self::Error> {
    let action_id = action.id.clone();
    let expires_at_ns = match action.expires_at {
      Some(dt) => {
        let ns_opt = dt.and_utc().timestamp_nanos_opt();
        if ns_opt.is_none() {
          return Err(JsError::new(&format!(
            "Action '{}' expiration timestamp is out of valid range for conversion to nanoseconds",
            action_id
          )));
        }
        ns_opt
      }
      None => None,
    };

    Ok(Self {
      id: action.id,
      label: action.label,
      image_url: action.image_url,
      style: action.style.map(|s| s.into()),
      expires_at_ns,
    })
  }
}

impl From<Action> for xmtp_content_types::actions::Action {
  fn from(action: Action) -> Self {
    let expires_at = match action.expires_at_ns {
      Some(ns) => {
        let dt = DateTime::from_timestamp_nanos(ns).naive_utc();
        Some(dt)
      }
      None => None,
    };

    Self {
      id: action.id,
      label: action.label,
      image_url: action.image_url,
      style: action.style.map(|s| s.into()),
      expires_at,
    }
  }
}

#[wasm_bindgen_numbered_enum]
pub enum ActionStyle {
  Primary = 0,
  Secondary = 1,
  Danger = 2,
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

#[wasm_bindgen(js_name = "contentTypeActions")]
pub fn content_type_actions() -> ContentTypeId {
  ActionsCodec::content_type().into()
}

#[wasm_bindgen(js_name = "encodeActions")]
pub fn encode_actions(actions: Actions) -> Result<EncodedContent, JsError> {
  Ok(
    ActionsCodec::encode(actions.into())
      .map_err(|e| JsError::new(&format!("{}", e)))?
      .into(),
  )
}
