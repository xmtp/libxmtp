//! This crate only compiles for webassembly
#![recursion_limit = "256"]

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
    pub mod message_delivery;
    pub mod opfs;
    pub mod permissions;
    pub mod server_configuration;
    pub mod signatures;
    pub mod streams;
    mod user_preferences;
    pub mod errors;
    pub use errors::*;
    #[cfg(any(test, feature = "test-utils"))]
    pub mod tests;
    #[cfg(any(test, feature = "test-utils"))]
    mod builder_test;
}

pub fn lib() {
  if !cfg!(target_os = "unknown") && !cfg!(target_family = "wasm") {
    panic!("only webassembly is supported")
  }
}

#[cfg(all(doctest, target_arch = "wasm32"))]
/// Notification APIs must stay absent from the browser binding.
///
/// ```compile_fail
/// use bindings_wasm::client::Client;
/// let _ = Client::enable_notifications;
/// ```
///
/// ```compile_fail
/// use bindings_wasm::client::Client;
/// let _ = Client::disable_notifications;
/// ```
///
/// ```compile_fail
/// use bindings_wasm::client::Client;
/// let _ = Client::notification_state;
/// ```
///
/// ```compile_fail
/// use bindings_wasm::conversation::Conversation;
/// let _ = Conversation::set_notifications;
/// ```
///
/// ```compile_fail
/// use bindings_wasm::conversation::Conversation;
/// let _ = Conversation::notifications_enabled;
/// ```
///
/// ```compile_fail
/// use bindings_wasm::client::NotificationChannel;
/// ```
///
/// ```compile_fail
/// use bindings_wasm::client::NotificationConfig;
/// ```
///
/// ```compile_fail
/// use bindings_wasm::client::NotificationState;
/// ```
///
/// ```compile_fail
/// use bindings_wasm::conversation::NotificationOverride;
/// ```
struct WasmNotificationApiIsUnavailable;

#[cfg(all(doctest, target_arch = "wasm32"))]
/// Browser bindings must continue to export the client and conversation types.
///
/// ```no_run
/// use bindings_wasm::{client::Client, conversation::Conversation};
///
/// fn uses_browser_types(_: &Client, _: &Conversation) {}
/// ```
struct WasmClientAndConversationAreExported;
