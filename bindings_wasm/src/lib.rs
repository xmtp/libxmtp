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

/// Converts an error implementing ErrorCode to a JsError with the format:
/// `[ErrorType::Variant] error message`
///
/// This provides consistent error formatting across all WASM bindings,
/// allowing JavaScript code to programmatically identify error types.
#[inline]
pub fn error<E: std::error::Error + ErrorCode>(e: E) -> JsError {
  JsError::new(&format!("[{}] {}", e.error_code(), e))
}

/// Converts any error to a JsError without error code prefix.
/// Use this for errors that don't implement ErrorCode.
#[inline]
pub fn simple_error<E: std::error::Error>(e: E) -> JsError {
  JsError::new(&e.to_string())
}

/// Converts a Rust value into a [`JsValue`].
pub(crate) fn to_value<T: serde::ser::Serialize + ?Sized>(
  value: &T,
) -> Result<JsValue, serde_wasm_bindgen::Error> {
  value.serialize(&Serializer::new().serialize_large_number_types_as_bigints(true))
}

#[cfg(any(test, feature = "test-utils"))]
pub mod tests;
