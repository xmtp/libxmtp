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
xmtp_common::if_test! {
  pub mod test_utils;
}

use napi::bindgen_prelude::Error;

pub use xmtp_common::HexError;

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
  E: std::error::Error + xmtp_common::ErrorCode;

impl<T: std::error::Error + xmtp_common::ErrorCode> std::fmt::Display for ErrorWrapper<T> {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
    write!(f, "{}", self.0)
  }
}

impl<T> From<T> for ErrorWrapper<T>
where
  T: std::error::Error + xmtp_common::ErrorCode,
{
  fn from(err: T) -> ErrorWrapper<T> {
    ErrorWrapper(err)
  }
}

impl<T: std::error::Error + xmtp_common::ErrorCode> From<ErrorWrapper<T>>
  for napi::bindgen_prelude::Error
{
  fn from(e: ErrorWrapper<T>) -> napi::bindgen_prelude::Error {
    let code = e.0.error_code();
    let message = e.0.to_string();
    Error::from_reason(format!("[{}] {}", code, message))
  }
}
