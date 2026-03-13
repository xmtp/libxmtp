#![recursion_limit = "256"]
#![warn(clippy::unwrap_used)]

#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
pub mod builder;
#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
pub mod client;
#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
mod consent_state;
#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
pub mod content_types;
#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
pub mod conversation;
#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
pub mod conversations;
#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
pub mod device_sync;
#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
pub mod hmac_key;
#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
mod identity;
#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
pub mod inbox_id;
#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
mod inbox_state;
#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
mod messages;
#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
pub mod native;
#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
mod permissions;
#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
mod signatures;
#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
pub mod stats;
#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
mod streams;
#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
pub use native::*;

#[cfg(all(not(all(target_family = "wasm", target_os = "unknown")), test))]
pub mod test_utils;
