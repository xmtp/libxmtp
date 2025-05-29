macro_rules! wasm_modules {
    ($($scope:vis mod $mod_name:ident;)*) => {
        $(
            #[cfg(all(target_family = "wasm", target_os = "unknown"))]
            $scope mod $mod_name;
        )*
    };

}

wasm_modules! {
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
    pub mod opfs;
    pub mod permissions;
    pub mod signatures;
    pub mod streams;
    mod user_preferences;
}

#[cfg(all(target_family = "wasm", target_os = "unknown"))]
pub mod util {
  pub fn error(e: impl std::error::Error) -> JsError {
    JsError::new(&format!("{}", e))
  }
  use serde_wasm_bindgen::Serializer;
  use wasm_bindgen::{JsError, JsValue};

  /// Converts a Rust value into a [`JsValue`].
  pub fn to_value<T: serde::ser::Serialize + ?Sized>(
    value: &T,
  ) -> Result<JsValue, serde_wasm_bindgen::Error> {
    value.serialize(&Serializer::new().serialize_large_number_types_as_bigints(true))
  }
}
#[cfg(all(target_family = "wasm", target_os = "unknown"))]
pub use util::*;

#[cfg(all(test, target_family = "wasm", target_os = "unknown"))]
mod tests;
