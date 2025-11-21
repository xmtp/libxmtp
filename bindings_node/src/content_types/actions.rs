use chrono::DateTime;
use napi::bindgen_prelude::{Error, Result, Uint8Array};
use napi_derive::napi;
use prost::Message;
use xmtp_content_types::{ContentCodec, actions::ActionsCodec};
use xmtp_proto::xmtp::mls::message_contents::EncodedContent;

use crate::ErrorWrapper;

#[derive(Clone)]
#[napi(object)]
pub struct Actions {
  pub id: String,
  pub description: String,
  pub actions: Vec<Action>,
  pub expires_at_ns: Option<i64>,
}

impl TryFrom<xmtp_content_types::actions::Actions> for Actions {
  type Error = Error;

  fn try_from(
    actions: xmtp_content_types::actions::Actions,
  ) -> std::result::Result<Self, Self::Error> {
    let actions_id = actions.id.clone();
    let expires_at_ns = match actions.expires_at {
      Some(dt) => {
        let ns_opt = dt.and_utc().timestamp_nanos_opt();
        if ns_opt.is_none() {
          return Err(Error::from_reason(format!(
            "Actions '{}' expiration timestamp is out of valid range for conversion to nanoseconds",
            actions_id
          )));
        }
        ns_opt
      }
      None => None,
    };

    let converted_actions: std::result::Result<Vec<_>, _> =
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
    let expires_at = actions
      .expires_at_ns
      .map(|ns| DateTime::from_timestamp_nanos(ns).naive_utc());

    xmtp_content_types::actions::Actions {
      id: actions.id,
      description: actions.description,
      actions: actions.actions.into_iter().map(|a| a.into()).collect(),
      expires_at,
    }
  }
}

#[derive(Clone)]
#[napi(object)]
pub struct Action {
  pub id: String,
  pub label: String,
  pub image_url: Option<String>,
  pub style: Option<ActionStyle>,
  pub expires_at_ns: Option<i64>,
}

impl TryFrom<xmtp_content_types::actions::Action> for Action {
  type Error = Error;

  fn try_from(
    action: xmtp_content_types::actions::Action,
  ) -> std::result::Result<Self, Self::Error> {
    let action_id = action.id.clone();
    let expires_at_ns = match action.expires_at {
      Some(dt) => {
        let ns_opt = dt.and_utc().timestamp_nanos_opt();
        if ns_opt.is_none() {
          return Err(Error::from_reason(format!(
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
    let expires_at = action
      .expires_at_ns
      .map(|ns| DateTime::from_timestamp_nanos(ns).naive_utc());

    xmtp_content_types::actions::Action {
      id: action.id,
      label: action.label,
      image_url: action.image_url,
      style: action.style.map(|s| s.into()),
      expires_at,
    }
  }
}

#[derive(Clone)]
#[napi]
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

#[napi]
pub fn encode_actions(actions: Actions) -> Result<Uint8Array> {
  // Convert Actions and use ActionsCodec to encode
  let encoded = ActionsCodec::encode(actions.into()).map_err(ErrorWrapper::from)?;

  // Encode the EncodedContent to bytes
  let mut buf = Vec::new();
  encoded.encode(&mut buf).map_err(ErrorWrapper::from)?;

  Ok(Uint8Array::from(buf.as_slice()))
}

#[napi]
pub fn decode_actions(bytes: Uint8Array) -> Result<Actions> {
  // Decode bytes into EncodedContent
  let encoded_content =
    EncodedContent::decode(bytes.to_vec().as_slice()).map_err(ErrorWrapper::from)?;

  // Use ActionsCodec to decode into Actions and convert to Actions
  let actions = ActionsCodec::decode(encoded_content).map_err(ErrorWrapper::from)?;

  actions.try_into()
}
