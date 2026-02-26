//! This crate only compiles for webassembly

xmtp_common::if_wasm! {
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
    pub mod errors;
    pub use errors::*;
    #[cfg(any(test, feature = "test-utils"))]
    pub mod tests;
}

pub fn lib() {
  if !cfg!(target_os = "unknown") && !cfg!(target_family = "wasm") {
    panic!("only webassembly is supported")
  }
}
