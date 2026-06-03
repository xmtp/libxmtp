#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod file;
// `fmt` (the plain stdout layer) is only used by the native `install`; on wasm
// the browser console layer in `web` takes its place.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod fmt;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod native;
#[cfg(target_arch = "wasm32")]
pub(crate) mod web;
