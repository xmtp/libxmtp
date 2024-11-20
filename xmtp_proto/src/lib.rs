#[allow(clippy::all)]
mod generated {
    include!("gen/mod.rs");
}
pub use generated::*;

mod error;
pub use error::*;

#[cfg(feature = "xmtp-message_api-v1")]
pub mod api_client;

#[cfg(feature = "convert")]
pub mod convert;
#[cfg(feature = "convert")]
pub mod types;
#[cfg(feature = "convert")]
pub mod v4_utils;

#[cfg(test)]
pub mod test {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);
}
