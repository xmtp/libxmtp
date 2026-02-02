pub use bindings_wasm_macros::wasm_bindgen_numbered_enum;

pub mod client;
pub mod consent_state;
pub mod content_types;
pub mod conversation;
pub mod conversations;
pub mod device_sync;
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

#[allow(dead_code)]
fn error(e: impl std::error::Error) -> JsError {
  JsError::new(&format!("{}", e))
}
use serde_wasm_bindgen::Serializer;
use wasm_bindgen::{JsError, JsValue};

/// Converts a Rust value into a [`JsValue`].
pub(crate) fn to_value<T: serde::ser::Serialize + ?Sized>(
  value: &T,
) -> Result<JsValue, serde_wasm_bindgen::Error> {
  value.serialize(&Serializer::new().serialize_large_number_types_as_bigints(true))
}

#[cfg(any(test, feature = "test-utils"))]
pub mod tests;
