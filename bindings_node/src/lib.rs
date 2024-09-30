#![recursion_limit = "256"]
#![warn(clippy::unwrap_used)]

mod consent_state;
mod conversations;
mod encoded_content;
mod groups;
mod inbox_state;
mod messages;
pub mod mls_client;
mod permissions;
mod streams;

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
