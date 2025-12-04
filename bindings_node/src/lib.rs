#![recursion_limit = "256"]
#![warn(clippy::unwrap_used)]

pub mod client;
mod consent_state;
pub mod content_types;
mod conversation;
mod conversations;
mod encoded_content;
pub mod enriched_message;
mod identity;
pub mod inbox_id;
mod inbox_state;
mod message;
mod permissions;
mod signatures;
mod streams;
#[cfg(feature = "test-utils")]
pub mod error_helpers;
xmtp_common::if_test! {
  pub mod test_utils;
}

use napi::bindgen_prelude::Error;
use xmtp_common::ErrorCode;

/// Wrapper over any error
/// to make most error handling in napi cleaner
#[derive(Debug)]
pub struct ErrorWrapper<E>(E)
where
  E: std::error::Error + ErrorCode;

impl<T: std::error::Error + ErrorCode + 'static> std::fmt::Display for ErrorWrapper<T> {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
    write!(f, "[{}] {}", self.0.error_code(), self.0)
  }
}

impl<T: std::error::Error + ErrorCode + 'static> From<T> for ErrorWrapper<T> {
  fn from(err: T) -> ErrorWrapper<T> {
    ErrorWrapper(err)
  }
}

impl<T: std::error::Error + ErrorCode + 'static> From<ErrorWrapper<T>> for napi::bindgen_prelude::Error {
  fn from(e: ErrorWrapper<T>) -> napi::bindgen_prelude::Error {
    Error::from_reason(e.to_string())
  }
}
