pub mod client;
pub mod consent_state;
pub mod content_types;
pub mod conversation;
pub mod conversations;
pub mod encoded_content;
pub mod identity;
pub mod inbox_id;
pub mod inbox_state;
pub mod messages;
pub mod permissions;
pub mod signatures;
pub mod streams;

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
