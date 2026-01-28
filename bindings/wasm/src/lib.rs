pub use bindings_wasm_macros::wasm_bindgen_numbered_enum;

pub mod client;
pub mod consent_state;
pub mod content_types;
pub mod conversation;
pub mod conversations;
pub mod encoded_content;
pub mod enriched_message;
pub mod identity;
pub mod inbox_id;
pub mod inbox_state;
pub mod messages;
pub mod opfs;
pub mod permissions;
pub mod signatures;
pub mod streams;
mod user_preferences;

use serde_wasm_bindgen::Serializer;
use wasm_bindgen::{JsError, JsValue};
use xmtp_common::ErrorCode;

#[allow(dead_code)]
fn error(e: impl std::error::Error) -> JsError {
  JsError::new(&format!("{}", e))
}

/// Wrapper for errors that implement ErrorCode trait.
/// Prefixes the error message with the error code.
///
/// Format: `[ErrorType::Variant] error message`
///
/// JavaScript usage:
/// ```js
/// import { parseXmtpError } from '@aspect-build/xmtp-js-sdk/error';
///
/// try {
///   await client.doSomething();
/// } catch (e) {
///   const err = parseXmtpError(e);
///   console.log(err.code);    // "ErrorType::Variant"
///   console.log(err.message); // "[ErrorType::Variant] error message"
/// }
/// ```
#[derive(Debug)]
pub struct ErrorWrapper<E>(pub E)
where
  E: std::error::Error + ErrorCode;

impl<T: std::error::Error + ErrorCode> std::fmt::Display for ErrorWrapper<T> {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
    write!(f, "{}", self.0)
  }
}

impl<T> From<T> for ErrorWrapper<T>
where
  T: std::error::Error + ErrorCode,
{
  fn from(err: T) -> ErrorWrapper<T> {
    ErrorWrapper(err)
  }
}

impl<T: std::error::Error + ErrorCode> From<ErrorWrapper<T>> for JsError {
  fn from(e: ErrorWrapper<T>) -> JsError {
    let code = e.0.error_code();
    let message = e.0.to_string();
    JsError::new(&format!("[{}] {}", code, message))
  }
}

/// Converts a Rust value into a [`JsValue`].
pub(crate) fn to_value<T: serde::ser::Serialize + ?Sized>(
  value: &T,
) -> Result<JsValue, serde_wasm_bindgen::Error> {
  value.serialize(&Serializer::new().serialize_large_number_types_as_bigints(true))
}

#[cfg(any(test, feature = "test-utils"))]
pub mod tests;
