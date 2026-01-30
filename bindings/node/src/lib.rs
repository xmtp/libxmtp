#![recursion_limit = "256"]
#![warn(clippy::unwrap_used)]

pub mod client;
mod consent_state;
pub mod content_types;
pub mod conversation;
pub mod conversations;
pub mod device_sync;
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
use xmtp_common::ErrorCode;

/// Wrapper for errors that implement ErrorCode trait.
/// Prefixes the error message with the error code.
///
/// Format: `[ErrorType::Variant] error message`
///
/// JavaScript usage:
/// ```js
/// try {
///   await client.doSomething();
/// } catch (e) {
///   console.log(e.message); // "[ErrorType::Variant] error message"
/// }
/// ```
#[derive(Debug)]
pub struct ErrorWrapper<E>(pub E)
where
  E: ErrorCode;

impl<T: ErrorCode> std::fmt::Display for ErrorWrapper<T> {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    self.0.fmt(f)
  }
}

impl<T> From<T> for ErrorWrapper<T>
where
  T: ErrorCode,
{
  fn from(err: T) -> ErrorWrapper<T> {
    ErrorWrapper(err)
  }
}

impl<T: ErrorCode> From<ErrorWrapper<T>> for napi::bindgen_prelude::Error {
  fn from(e: ErrorWrapper<T>) -> napi::bindgen_prelude::Error {
    let code = e.0.error_code();
    let message = e.0.to_string();
    Error::from_reason(format!("[{}] {}", code, message))
  }
}
