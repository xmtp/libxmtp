use chrono::DateTime;
use napi::bindgen_prelude::{Result, Uint8Array};
use napi_derive::napi;
use prost::Message;
use xmtp_content_types::{ContentCodec, actions::ActionsCodec};
use xmtp_proto::xmtp::mls::message_contents::EncodedContent;

use crate::ErrorWrapper;

#[derive(Debug)]
struct TimestampValidationError {
  timestamp: i64,
  context: &'static str,
}

impl std::fmt::Display for TimestampValidationError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(
      f,
      "{} timestamp {} is out of valid range and was clamped",
      self.context, self.timestamp
    )
  }
}

impl std::error::Error for TimestampValidationError {}

#[derive(Clone)]
#[napi(object)]
pub struct Actions {
  pub id: String,
  pub description: String,
  pub actions: Vec<Action>,
  pub expires_at_ns: Option<i64>,
}

impl From<xmtp_content_types::actions::Actions> for Actions {
  fn from(actions: xmtp_content_types::actions::Actions) -> Self {
    Self {
      id: actions.id,
      description: actions.description,
      actions: actions.actions.into_iter().map(|a| a.into()).collect(),
      expires_at_ns: actions
        .expires_at
        .and_then(|dt| dt.and_utc().timestamp_nanos_opt()),
    }
  }
}

impl TryFrom<Actions> for xmtp_content_types::actions::Actions {
  type Error = napi::Error<napi::Status>;

  fn try_from(actions: Actions) -> std::result::Result<Self, Self::Error> {
    let expires_at = match actions.expires_at_ns {
      Some(ns) => {
        // Create DateTime and immediately validate it didn't clamp
        let dt = DateTime::from_timestamp_nanos(ns).naive_utc();
        let roundtrip_ns = dt.and_utc().timestamp_nanos_opt();
        if roundtrip_ns != Some(ns) {
          return Err(
            ErrorWrapper::from(TimestampValidationError {
              timestamp: ns,
              context: "Actions",
            })
            .into(),
          );
        }
        Some(dt)
      }
      None => None,
    };

    let actions_vec = actions
      .actions
      .into_iter()
      .map(|a| a.try_into())
      .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(xmtp_content_types::actions::Actions {
      id: actions.id,
      description: actions.description,
      actions: actions_vec,
      expires_at,
    })
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

impl From<xmtp_content_types::actions::Action> for Action {
  fn from(action: xmtp_content_types::actions::Action) -> Self {
    Self {
      id: action.id,
      label: action.label,
      image_url: action.image_url,
      style: action.style.map(|s| s.into()),
      expires_at_ns: action
        .expires_at
        .and_then(|dt| dt.and_utc().timestamp_nanos_opt()),
    }
  }
}

impl TryFrom<Action> for xmtp_content_types::actions::Action {
  type Error = napi::Error<napi::Status>;

  fn try_from(action: Action) -> std::result::Result<Self, Self::Error> {
    let expires_at = match action.expires_at_ns {
      Some(ns) => {
        // Create DateTime and immediately validate it didn't clamp
        let dt = DateTime::from_timestamp_nanos(ns).naive_utc();
        let roundtrip_ns = dt.and_utc().timestamp_nanos_opt();
        if roundtrip_ns != Some(ns) {
          return Err(
            ErrorWrapper::from(TimestampValidationError {
              timestamp: ns,
              context: "Action",
            })
            .into(),
          );
        }
        Some(dt)
      }
      None => None,
    };

    Ok(xmtp_content_types::actions::Action {
      id: action.id,
      label: action.label,
      image_url: action.image_url,
      style: action.style.map(|s| s.into()),
      expires_at,
    })
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
  // Use ActionsCodec to encode the actions
  let actions: xmtp_content_types::actions::Actions = actions.try_into()?;
  let encoded = ActionsCodec::encode(actions).map_err(ErrorWrapper::from)?;

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
  let actions = ActionsCodec::decode(encoded_content)
    .map(Into::into)
    .map_err(ErrorWrapper::from)?;

  Ok(actions)
}
