pub mod client;
pub mod consent_state;
pub mod conversation;
pub mod conversations;
pub mod encoded_content;
pub mod inbox_id;
pub mod inbox_state;
pub mod messages;
pub mod permissions;
pub mod signatures;
pub mod streams;

fn error(e: impl std::error::Error) -> wasm_bindgen::JsError {
  wasm_bindgen::JsError::new(&format!("{}", e))
}
