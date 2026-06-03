pub(crate) mod fmt;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod file;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod native;
#[cfg(target_arch = "wasm32")]
pub(crate) mod web;
