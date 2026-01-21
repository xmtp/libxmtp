#![recursion_limit = "256"]
#![warn(clippy::unwrap_used)]

pub mod client;
mod consent_state;
pub mod content_types;
pub mod conversation;
pub mod conversations;
pub mod hmac_key;
mod identity;
pub mod inbox_id;
mod inbox_state;
mod messages;
mod permissions;
mod signatures;
pub mod stats;
mod streams;
xmtp_common::if_test! {
  pub mod test_utils;
}

use napi::bindgen_prelude::Error;

/// Wrapper over any error
/// to make most error handling in napi cleaner
#[derive(Debug)]
pub struct ErrorWrapper<E>(E)
where
  E: std::error::Error;

impl<T: std::error::Error> std::fmt::Display for ErrorWrapper<T> {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
    write!(f, "{}", self.0)
  }
}

impl<T> From<T> for ErrorWrapper<T>
where
  T: std::error::Error,
{
  fn from(err: T) -> ErrorWrapper<T> {
    ErrorWrapper(err)
  }
}

impl<T: std::error::Error> From<ErrorWrapper<T>> for napi::bindgen_prelude::Error {
  fn from(e: ErrorWrapper<T>) -> napi::bindgen_prelude::Error {
    Error::from_reason(e.to_string())
  }
}
